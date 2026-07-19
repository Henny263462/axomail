//! Integration tests for the storage layer, exercised against in-memory
//! and temp-file SQLite databases.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountAuth, MailboxRole, MessageFlags, SyncOutcome, ThreadingHeaders};
use axomail_storage::{MessageCursor, MessagePage, Store};
use common::{
    pop3_envelope_with_threading, sample_account, sample_mailbox, sample_message,
    sample_oauth_account,
};

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
async fn oauth_account_round_trips_with_config_columns() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.accounts();

    let created = repo
        .insert(&sample_oauth_account("Gmail", "klaus@gmail.com"))
        .await
        .unwrap();

    let fetched = repo.get(created.id).await.unwrap().expect("account exists");
    assert_eq!(fetched, created);
    // The mechanism and its non-secret config survive the round trip.
    let AccountAuth::OAuth2(config) = &fetched.auth else {
        panic!("expected an OAuth2 account");
    };
    assert_eq!(config.client_id.as_str(), "123.apps.googleusercontent.com");
    assert_eq!(
        config.token_url.as_str(),
        "https://oauth2.googleapis.com/token"
    );
    assert_eq!(
        config.scopes.as_space_delimited(),
        "https://mail.google.com/"
    );
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
    assert!(repo.update_flags(message.id, &updated).await.unwrap());

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

/// Threading identity carried by an envelope (POP3 full-header scan) is
/// persisted at ingest and reconstructed on read (migration 0006).
#[tokio::test]
async fn threading_identity_round_trips_through_ingest() {
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

    let headers = ThreadingHeaders {
        message_id: Some("reply-2@example.org".to_owned()),
        in_reply_to: vec!["root-0@example.org".to_owned()],
        references: vec![
            "root-0@example.org".to_owned(),
            "mid-1@example.org".to_owned(),
        ],
    };
    let outcome = SyncOutcome {
        uid_validity: None,
        highest_uid: None,
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![pop3_envelope_with_threading("uidl-1", "Re: Hello", headers)],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    assert_eq!(
        store
            .ingest()
            .apply(mailbox.id, &outcome)
            .await
            .unwrap()
            .inserted,
        1
    );

    let page = store
        .messages()
        .list_page(mailbox.id, None, 10)
        .await
        .unwrap();
    let stored = &page.items[0].threading;
    assert_eq!(stored.message_id.as_deref(), Some("reply-2@example.org"));
    assert_eq!(stored.in_reply_to, vec!["root-0@example.org".to_owned()]);
    assert_eq!(
        stored.references,
        vec![
            "root-0@example.org".to_owned(),
            "mid-1@example.org".to_owned()
        ]
    );
    assert_eq!(stored.parent_id(), Some("mid-1@example.org"));
}

#[tokio::test]
async fn ingest_persists_and_reuses_highest_modseq() {
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

    // A first CONDSTORE pass records the mailbox mod-sequence cursor.
    let first = SyncOutcome {
        uid_validity: Some(42),
        highest_uid: Some(7),
        highest_modseq: Some(1000),
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox.id, &first).await.unwrap();

    let reused = store.sync_state().get(mailbox.id).await.unwrap();
    assert_eq!(reused.uid_validity, Some(42));
    assert_eq!(reused.highest_uid, Some(7));
    assert_eq!(
        reused.highest_modseq,
        Some(1000),
        "the mod-sequence cursor survives to the next pass"
    );

    // A later pass advances it; a NOMODSEQ pass would clear it.
    let second = SyncOutcome {
        highest_modseq: Some(2500),
        ..first.clone()
    };
    store.ingest().apply(mailbox.id, &second).await.unwrap();
    assert_eq!(
        store
            .sync_state()
            .get(mailbox.id)
            .await
            .unwrap()
            .highest_modseq,
        Some(2500)
    );

    let cleared = SyncOutcome {
        highest_modseq: None,
        ..first
    };
    store.ingest().apply(mailbox.id, &cleared).await.unwrap();
    assert_eq!(
        store
            .sync_state()
            .get(mailbox.id)
            .await
            .unwrap()
            .highest_modseq,
        None,
        "a NOMODSEQ pass resets the cursor"
    );
}

/// The lazy backfill writes threading once; a message that already carries
/// it (or was ingested with it) is never overwritten.
#[tokio::test]
async fn backfill_threading_is_idempotent_and_first_write_wins() {
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
    // A local insert leaves the threading columns NULL (not yet extracted).
    let message = store
        .messages()
        .insert(&sample_message(mailbox.id, "hello", Some(1)))
        .await
        .unwrap();
    assert!(message.threading.is_empty());

    let first = ThreadingHeaders {
        message_id: Some("self@example.org".to_owned()),
        in_reply_to: vec!["parent@example.org".to_owned()],
        references: vec!["parent@example.org".to_owned()],
    };
    // First backfill populates the row.
    assert!(
        store
            .messages()
            .backfill_threading(message.id, &first)
            .await
            .unwrap()
    );

    // A second backfill with different headers is a guarded no-op.
    let second = ThreadingHeaders {
        message_id: Some("other@example.org".to_owned()),
        in_reply_to: Vec::new(),
        references: Vec::new(),
    };
    assert!(
        !store
            .messages()
            .backfill_threading(message.id, &second)
            .await
            .unwrap()
    );

    let fetched = store.messages().get(message.id).await.unwrap().unwrap();
    assert_eq!(
        fetched.threading.message_id.as_deref(),
        Some("self@example.org")
    );
    assert_eq!(
        fetched.threading.references,
        vec!["parent@example.org".to_owned()]
    );
}

/// `find_by_message_id` resolves within an account and ignores foreign
/// accounts and unknown ids.
#[tokio::test]
async fn find_by_message_id_is_account_scoped() {
    let store = Store::open_in_memory().await.unwrap();
    let account_a = store
        .accounts()
        .insert(&sample_account("A", "a@theferbers.eu"))
        .await
        .unwrap();
    let account_b = store
        .accounts()
        .insert(&sample_account("B", "b@theferbers.eu"))
        .await
        .unwrap();
    let mailbox_a = store
        .mailboxes()
        .upsert(&sample_mailbox(account_a.id, "INBOX"))
        .await
        .unwrap();
    let mailbox_b = store
        .mailboxes()
        .upsert(&sample_mailbox(account_b.id, "INBOX"))
        .await
        .unwrap();

    let headers = ThreadingHeaders {
        message_id: Some("shared@example.org".to_owned()),
        in_reply_to: Vec::new(),
        references: Vec::new(),
    };
    let outcome = |uidl: &str| SyncOutcome {
        uid_validity: None,
        highest_uid: None,
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![pop3_envelope_with_threading(
            uidl,
            "Shared",
            headers.clone(),
        )],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    store
        .ingest()
        .apply(mailbox_a.id, &outcome("a-1"))
        .await
        .unwrap();
    store
        .ingest()
        .apply(mailbox_b.id, &outcome("b-1"))
        .await
        .unwrap();

    let in_a = store
        .messages()
        .find_by_message_id(account_a.id, "shared@example.org")
        .await
        .unwrap();
    let in_b = store
        .messages()
        .find_by_message_id(account_b.id, "shared@example.org")
        .await
        .unwrap();
    assert!(in_a.is_some());
    assert!(in_b.is_some());
    assert_ne!(in_a, in_b, "each account resolves to its own row");

    // An unknown id under a real account misses.
    assert_eq!(
        store
            .messages()
            .find_by_message_id(account_a.id, "missing@example.org")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn remote_content_allow_list_round_trips_case_insensitively() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.remote_content();

    // Default: an unknown sender is not allowed.
    assert!(!repo.is_allowed("alice@example.org").await.unwrap());

    // Allowing is stored normalized; a differently-cased/spaced lookup matches.
    repo.set_allowed("  Alice@Example.ORG ", true)
        .await
        .unwrap();
    assert!(repo.is_allowed("alice@example.org").await.unwrap());
    assert!(repo.is_allowed("ALICE@EXAMPLE.ORG").await.unwrap());
    assert_eq!(
        repo.allowed_senders().await.unwrap(),
        vec!["alice@example.org"]
    );

    // Re-allowing is idempotent (no duplicate row, no error).
    repo.set_allowed("alice@example.org", true).await.unwrap();
    assert_eq!(repo.allowed_senders().await.unwrap().len(), 1);

    // Removing it returns to the blocked default; removing again is a no-op.
    repo.set_allowed("alice@example.org", false).await.unwrap();
    assert!(!repo.is_allowed("alice@example.org").await.unwrap());
    repo.set_allowed("alice@example.org", false).await.unwrap();
    assert!(repo.allowed_senders().await.unwrap().is_empty());

    // An empty sender is rejected for writes and never allowed for reads.
    assert!(repo.set_allowed("   ", true).await.is_err());
    assert!(!repo.is_allowed("").await.unwrap());
}
