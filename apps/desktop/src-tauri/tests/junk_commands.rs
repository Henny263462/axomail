//! Integration tests for the junk commands: mark-junk / not-junk (which train
//! the classifier, toggle the `$Junk` keyword, move the message, and queue the
//! write-back), run junk controls with server-header trust, and the settings
//! round-trip. Backed by an in-memory store and the never-connecting stub
//! engine, so enqueued ops stay put for inspection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, Keyword, MailboxId, MailboxName, MailboxRole, MessageFlags,
    MessageId, RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::junk::{
    get_junk_settings_impl, mark_junk_impl, not_junk_impl, run_junk_controls_impl,
    set_junk_settings_impl,
};
use axomail_desktop_lib::dto::JunkSettingsDto;
use axomail_storage::{
    JunkSettings, NewMailbox, NewMessage, NewTag, Store, raw_message_path, write_raw_atomically,
};
use common::provider::stub_engine;
use common::{secret_stores, seed_standard_folders, valid_input};
use tempfile::tempdir;

fn imap_id(uid: u32) -> RemoteMessageId {
    RemoteMessageId::Imap {
        uid,
        uid_validity: 1,
    }
}

async fn mailbox_by_role(store: &Store, account: AccountId, role: MailboxRole) -> MailboxId {
    store
        .mailboxes()
        .list(account)
        .await
        .unwrap()
        .into_iter()
        .find(|mailbox| mailbox.role == role)
        .expect("role folder")
        .id
}

/// Seeds an IMAP account with the standard folders plus a Junk folder.
async fn account_with_junk(store: &Store) -> (AccountId, MailboxId, MailboxId) {
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(store, &secrets, valid_input())
        .await
        .unwrap();
    let id = AccountId::new(account.id);
    seed_standard_folders(store, id).await;
    let junk = store
        .mailboxes()
        .upsert(&NewMailbox {
            account: id,
            name: MailboxName::new("Junk"),
            remote_name: MailboxName::new("Junk"),
            role: MailboxRole::Junk,
            uid_validity: None,
            uid_next: None,
        })
        .await
        .unwrap()
        .id;
    let inbox = mailbox_by_role(store, id, MailboxRole::Inbox).await;
    (id, inbox, junk)
}

async fn insert(
    store: &Store,
    mailbox: MailboxId,
    remote: Option<RemoteMessageId>,
    subject: &str,
    preview: &str,
) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: remote,
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: None,
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: Some(UtcTimestamp::from_unix_seconds(1)),
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: Some(preview.to_owned()),
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id
}

fn junk_keyword() -> Keyword {
    Keyword::parse("$Junk").unwrap()
}

async fn has_junk_keyword(store: &Store, message: MessageId) -> bool {
    store
        .tags()
        .tags_for_message(message)
        .await
        .unwrap()
        .iter()
        .any(|tag| tag.keyword == junk_keyword())
}

#[tokio::test]
async fn mark_junk_trains_moves_and_sets_the_keyword() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox, junk) = account_with_junk(&store).await;
    let message = insert(&store, inbox, Some(imap_id(1)), "cheap viagra", "buy now").await;
    let engine = stub_engine(&store);

    let outcome = mark_junk_impl(&store, &engine, vec![message.value()])
        .await
        .unwrap();

    // Filed to Junk locally, with an undo record pointing back to the Inbox.
    assert_eq!(get_mailbox(&store, message).await, junk);
    assert_eq!(outcome.moves.len(), 1);
    assert_eq!(outcome.moves[0].from_mailbox_id, inbox.value());
    // The $Junk keyword is applied locally.
    assert!(has_junk_keyword(&store, message).await);
    // The classifier learned one spam message; the score is pinned high.
    assert_eq!(store.junk().corpus(account).await.unwrap().spam_messages, 1);
    assert!(store.junk().score_of(message).await.unwrap().unwrap() >= 0.99);
    // Two write-back ops were queued (add-keyword + move); the stub never
    // connects, so nothing drains them.
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!(counts.pending + counts.inflight + counts.failed, 2);
}

#[tokio::test]
async fn not_junk_trains_moves_back_and_clears_the_keyword() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox, junk) = account_with_junk(&store).await;
    let message = insert(&store, junk, Some(imap_id(2)), "cheap viagra", "buy now").await;
    // Pre-mark it junk locally (as a prior filing would have).
    let tag = store
        .tags()
        .create(&NewTag {
            account,
            keyword: junk_keyword(),
            display_name: "Junk".to_owned(),
            color: None,
            sort_order: 0,
        })
        .await
        .unwrap();
    store.tags().add_message_tag(message, tag.id).await.unwrap();
    let engine = stub_engine(&store);

    let outcome = not_junk_impl(&store, &engine, vec![message.value()])
        .await
        .unwrap();

    // Restored to the Inbox and the $Junk keyword cleared.
    assert_eq!(get_mailbox(&store, message).await, inbox);
    assert_eq!(outcome.moves.len(), 1);
    assert_eq!(outcome.moves[0].from_mailbox_id, junk.value());
    assert!(!has_junk_keyword(&store, message).await);
    // Trained as ham; score pinned low.
    assert_eq!(store.junk().corpus(account).await.unwrap().ham_messages, 1);
    assert_eq!(store.junk().score_of(message).await.unwrap().unwrap(), 0.0);
}

#[tokio::test]
async fn run_junk_controls_trusts_a_server_spam_header() {
    let store = Store::open_in_memory().await.unwrap();
    let (_account, inbox, junk) = account_with_junk(&store).await;
    let raw_root = tempdir().unwrap();
    // A message the classifier has no opinion on (empty corpus → 0.5).
    let message = insert(
        &store,
        inbox,
        Some(imap_id(3)),
        "quarterly report",
        "figures",
    )
    .await;
    // Cache a raw whose header block flags it as spam.
    write_raw_atomically(
        &raw_message_path(raw_root.path(), message.value()),
        b"Subject: quarterly report\r\nX-Spam-Flag: YES\r\n\r\nbody",
    )
    .await
    .unwrap();
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.9,
            trust_server_headers: true,
            whitelist_contacts: true,
        })
        .await
        .unwrap();
    let engine = stub_engine(&store);

    let filed = run_junk_controls_impl(&store, &engine, raw_root.path(), inbox.value())
        .await
        .unwrap();

    assert_eq!(filed, 1, "the server-flagged message should be filed");
    assert_eq!(get_mailbox(&store, message).await, junk);
    assert_eq!(store.junk().score_of(message).await.unwrap().unwrap(), 1.0);
}

#[tokio::test]
async fn run_junk_controls_without_trust_keeps_unscored_mail() {
    let store = Store::open_in_memory().await.unwrap();
    let (_account, inbox, _junk) = account_with_junk(&store).await;
    let raw_root = tempdir().unwrap();
    let message = insert(
        &store,
        inbox,
        Some(imap_id(4)),
        "quarterly report",
        "figures",
    )
    .await;
    write_raw_atomically(
        &raw_message_path(raw_root.path(), message.value()),
        b"X-Spam-Flag: YES\r\n\r\nbody",
    )
    .await
    .unwrap();
    // Trust off: the empty-corpus score (0.5) stays below the threshold.
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.9,
            trust_server_headers: false,
            whitelist_contacts: true,
        })
        .await
        .unwrap();
    let engine = stub_engine(&store);

    let filed = run_junk_controls_impl(&store, &engine, raw_root.path(), inbox.value())
        .await
        .unwrap();

    assert_eq!(
        filed, 0,
        "no server-header trust and no training → nothing filed"
    );
    assert_eq!(get_mailbox(&store, message).await, inbox);
}

#[tokio::test]
async fn junk_settings_round_trip_through_the_command() {
    let store = Store::open_in_memory().await.unwrap();
    // Defaults: opt-in disabled.
    assert!(!get_junk_settings_impl(&store).await.unwrap().enabled);
    set_junk_settings_impl(
        &store,
        JunkSettingsDto {
            enabled: true,
            threshold: 0.7,
            trust_server_headers: false,
            whitelist_contacts: false,
        },
    )
    .await
    .unwrap();
    let loaded = get_junk_settings_impl(&store).await.unwrap();
    assert!(loaded.enabled);
    assert!((loaded.threshold - 0.7).abs() < f64::EPSILON);
    assert!(!loaded.trust_server_headers);
    assert!(!loaded.whitelist_contacts);
}

async fn get_mailbox(store: &Store, message: MessageId) -> MailboxId {
    store
        .messages()
        .get(message)
        .await
        .unwrap()
        .unwrap()
        .mailbox
}
