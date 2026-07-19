//! Integration tests for the storage layer, exercised against in-memory
//! and temp-file SQLite databases.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{MailboxRole, MessageFlags};
use axomail_storage::{MessageCursor, MessagePage, Store};
use common::{sample_account, sample_mailbox, sample_message};

/// Opens a fresh temp-file store, returning it and the temp dir to keep
/// the file alive for the test's duration.
async fn temp_store() -> (Store, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mail.db");
    let store = Store::open(&path).await.unwrap();
    (store, dir, path)
}

#[tokio::test]
async fn migrations_apply_cleanly_from_scratch() {
    // A freshly created temp-file database must migrate and be queryable.
    let (store, _dir, _path) = temp_store().await;
    assert!(store.accounts().list().await.unwrap().is_empty());

    // The in-memory constructor must migrate the same schema.
    let memory = Store::open_in_memory().await.unwrap();
    assert!(memory.accounts().list().await.unwrap().is_empty());
}

#[tokio::test]
async fn account_crud_round_trips() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.accounts();

    let created = repo
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();

    let fetched = repo.get(created.id).await.unwrap().expect("account exists");
    assert_eq!(fetched, created);
    assert_eq!(fetched.display_name, "Work");
    assert_eq!(fetched.email.as_str(), "klaus@theferbers.eu");
    assert_eq!(fetched.incoming.endpoint.host(), "imap.example.org");
    assert_eq!(fetched.outgoing.endpoint.port().get(), 465);

    let all = repo.list().await.unwrap();
    assert_eq!(all, vec![created.clone()]);

    assert!(repo.delete(created.id).await.unwrap());
    assert!(repo.get(created.id).await.unwrap().is_none());
    assert!(!repo.delete(created.id).await.unwrap());
}

#[tokio::test]
async fn account_update_changes_mutable_fields_and_preserves_the_rest() {
    use std::num::NonZeroU16;

    use axomail_core::{ConnectionSecurity, ServerEndpoint};
    use axomail_storage::AccountUpdate;

    let store = Store::open_in_memory().await.unwrap();
    let repo = store.accounts();
    let created = repo
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();

    // Swap both endpoints and both security modes relative to the fixture
    // (incoming Tls, outgoing StartTls) so every mutable field is exercised.
    let update = AccountUpdate {
        display_name: "Personal".to_owned(),
        incoming_endpoint: ServerEndpoint::new("imap2.example.org", NonZeroU16::new(143).unwrap())
            .unwrap(),
        incoming_security: ConnectionSecurity::StartTls,
        outgoing_endpoint: ServerEndpoint::new("smtp2.example.org", NonZeroU16::new(2525).unwrap())
            .unwrap(),
        outgoing_security: ConnectionSecurity::Tls,
    };
    let updated = repo.update(created.id, &update).await.unwrap();

    // Mutable fields changed as requested.
    assert_eq!(updated.display_name, "Personal");
    assert_eq!(updated.incoming.endpoint.host(), "imap2.example.org");
    assert_eq!(updated.incoming.endpoint.port().get(), 143);
    assert_eq!(updated.incoming.security, ConnectionSecurity::StartTls);
    assert_eq!(updated.outgoing.endpoint.host(), "smtp2.example.org");
    assert_eq!(updated.outgoing.endpoint.port().get(), 2525);
    assert_eq!(updated.outgoing.security, ConnectionSecurity::Tls);

    // Immutable fields are left exactly as stored.
    assert_eq!(updated.id, created.id);
    assert_eq!(updated.email, created.email);
    assert_eq!(updated.incoming.protocol, created.incoming.protocol);
    assert_eq!(updated.incoming.username, created.incoming.username);
    assert_eq!(updated.incoming.credential, created.incoming.credential);
    assert_eq!(updated.outgoing.username, created.outgoing.username);
    assert_eq!(updated.outgoing.credential, created.outgoing.credential);

    // The change is persisted, not merely reflected in the return value.
    let refetched = repo.get(created.id).await.unwrap().expect("account exists");
    assert_eq!(refetched, updated);
}

#[tokio::test]
async fn account_update_rejects_an_unknown_id() {
    use std::num::NonZeroU16;

    use axomail_core::{AccountId, ConnectionSecurity, MailError, ServerEndpoint};
    use axomail_storage::AccountUpdate;

    let store = Store::open_in_memory().await.unwrap();
    let update = AccountUpdate {
        display_name: "Ghost".to_owned(),
        incoming_endpoint: ServerEndpoint::new("imap.example.org", NonZeroU16::new(993).unwrap())
            .unwrap(),
        incoming_security: ConnectionSecurity::Tls,
        outgoing_endpoint: ServerEndpoint::new("smtp.example.org", NonZeroU16::new(465).unwrap())
            .unwrap(),
        outgoing_security: ConnectionSecurity::Tls,
    };

    let error = store
        .accounts()
        .update(AccountId::new(9999), &update)
        .await
        .unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));
}

#[tokio::test]
async fn mailbox_upsert_is_idempotent() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let repo = store.mailboxes();

    let first = repo
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    let second = repo
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();

    assert_eq!(first.id, second.id, "same key must reuse the id");
    assert_eq!(first.role, MailboxRole::Inbox);
    assert_eq!(store.mailboxes().list(account.id).await.unwrap().len(), 1);

    // A different name is a distinct mailbox.
    repo.upsert(&sample_mailbox(account.id, "Sent"))
        .await
        .unwrap();
    let mut names: Vec<String> = store
        .mailboxes()
        .list(account.id)
        .await
        .unwrap()
        .into_iter()
        .map(|mailbox| mailbox.name.to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["INBOX".to_owned(), "Sent".to_owned()]);
}

#[tokio::test]
async fn message_listing_paginates_with_stable_ordering() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    let repo = store.messages();

    // Dates chosen to exercise ties (200, 200), an undated pair, and
    // ordering across all of them.
    let dates = [
        Some(300),
        Some(200),
        Some(200),
        Some(100),
        None,
        None,
        Some(50),
    ];
    let mut ids = Vec::new();
    for (index, date) in dates.iter().enumerate() {
        let subject = format!("msg {index}");
        ids.push(
            repo.insert(&sample_message(mailbox.id, &subject, *date))
                .await
                .unwrap()
                .id,
        );
    }

    // Expected order: date DESC, then id DESC as the tiebreaker; undated
    // messages sort last.
    let expected = vec![ids[0], ids[2], ids[1], ids[3], ids[6], ids[5], ids[4]];

    let mut seen = Vec::new();
    let mut cursor: Option<MessageCursor> = None;
    let mut pages = 0;
    loop {
        let page: MessagePage = repo.list_page(mailbox.id, cursor, 3).await.unwrap();
        pages += 1;
        seen.extend(page.items.iter().map(|envelope| envelope.id));
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert!(pages > 2, "expected more than two pages, got {pages}");
    assert_eq!(seen, expected);

    // Recipients survive the round-trip.
    let first = repo.get(expected[0]).await.unwrap().unwrap();
    assert_eq!(first.from.len(), 1);
    assert_eq!(first.from[0].display_name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(first.to[0].email.as_str(), "klaus@theferbers.eu");
}

#[tokio::test]
async fn message_flags_update() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    let repo = store.messages();
    let message = repo
        .insert(&sample_message(mailbox.id, "hello", Some(1)))
        .await
        .unwrap();
    assert_eq!(message.flags, MessageFlags::default());

    let updated = MessageFlags {
        seen: true,
        flagged: true,
        ..MessageFlags::default()
    };
    assert!(repo.update_flags(message.id, updated).await.unwrap());

    let fetched = repo.get(message.id).await.unwrap().unwrap();
    assert_eq!(fetched.flags, updated);
}

#[tokio::test]
async fn deleting_an_account_cascades() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    let message = store
        .messages()
        .insert(&sample_message(mailbox.id, "hello", Some(1)))
        .await
        .unwrap();

    assert!(store.accounts().delete(account.id).await.unwrap());

    // The mailbox and message must be gone via ON DELETE CASCADE.
    assert!(store.mailboxes().list(account.id).await.unwrap().is_empty());
    assert!(store.messages().get(message.id).await.unwrap().is_none());
    let empty_page = store
        .messages()
        .list_page(mailbox.id, None, 10)
        .await
        .unwrap();
    assert!(empty_page.items.is_empty());
}

#[tokio::test]
async fn settings_round_trip() {
    let store = Store::open_in_memory().await.unwrap();
    let settings = store.settings();

    assert!(settings.get("theme").await.unwrap().is_none());
    settings.set("theme", "dark").await.unwrap();
    assert_eq!(
        settings.get("theme").await.unwrap().as_deref(),
        Some("dark")
    );

    // Overwrite is an upsert, not a duplicate.
    settings.set("theme", "light").await.unwrap();
    assert_eq!(
        settings.get("theme").await.unwrap().as_deref(),
        Some("light")
    );
}

#[tokio::test]
async fn credential_column_stores_only_the_reference() {
    // Insert through the public API, then read the raw columns with an
    // independent connection to prove only the reference strings — never
    // a secret — are persisted.
    let (store, _dir, path) = temp_store().await;
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();

    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&path);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let (incoming, outgoing): (String, String) = sqlx::query_as(
        "SELECT incoming_credential_ref, outgoing_credential_ref FROM accounts WHERE id = ?",
    )
    .bind(account.id.value())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(incoming, "axomail/account/incoming");
    assert_eq!(outgoing, "axomail/account/outgoing");
    // The references carry no secret material.
    for value in [&incoming, &outgoing] {
        assert!(!value.to_lowercase().contains("password"));
        assert!(!value.to_lowercase().contains("secret"));
    }
    pool.close().await;
}

#[tokio::test]
async fn cursor_serializes_and_round_trips() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    for index in 0..3 {
        store
            .messages()
            .insert(&sample_message(
                mailbox.id,
                &format!("m{index}"),
                Some(index),
            ))
            .await
            .unwrap();
    }

    let page = store
        .messages()
        .list_page(mailbox.id, None, 2)
        .await
        .unwrap();
    let cursor = page.next.expect("a second page exists");

    // JSON round-trip (serde) and opaque-token round-trip must agree.
    let json = serde_json::to_string(&cursor).unwrap();
    let from_json: MessageCursor = serde_json::from_str(&json).unwrap();
    assert_eq!(from_json, cursor);
    let from_token = MessageCursor::parse_token(&cursor.token()).unwrap();
    assert_eq!(from_token, cursor);

    assert!(MessageCursor::parse_token("not-a-cursor").is_err());
}
