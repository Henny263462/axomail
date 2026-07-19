//! Integration tests for the Trash lifecycle commands. Empty Trash removes the
//! local rows and raw bodies (the server expunge is proven at the adapter
//! layer); Compact reclaims orphaned raw blobs; the per-account empty-on-exit
//! setting round-trips.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxRole, MessageFlags, MessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::trash::{compact_storage_impl, empty_trash_impl};
use axomail_desktop_lib::settings::{empty_trash_on_exit_enabled, set_empty_trash_on_exit_enabled};
use axomail_storage::{NewMessage, Store, raw_message_path, read_raw, write_raw_atomically};
use common::provider::StubFactory;
use common::{pop3_account_input, secret_stores, seed_standard_folders, valid_input};
use tempfile::tempdir;

/// The id of an account's mailbox with `role`.
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

/// Inserts a local-only message into `mailbox`.
async fn insert(store: &Store, mailbox: MailboxId) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: None,
        subject: Some("hello".to_owned()),
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
        preview: None,
        has_raw: true,
    };
    store.messages().insert(&message).await.unwrap().id
}

#[tokio::test]
async fn empty_trash_deletes_local_rows_and_raw_bodies() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    // A POP3 account: Empty Trash skips the server step and purges locally.
    let account = AccountId::new(
        create_account_impl(&store, &secrets, pop3_account_input(995))
            .await
            .unwrap()
            .id,
    );
    seed_standard_folders(&store, account).await;
    let trash = mailbox_by_role(&store, account, MailboxRole::Trash).await;
    let dir = tempdir().unwrap();

    let m1 = insert(&store, trash).await;
    let m2 = insert(&store, trash).await;
    for id in [m1, m2] {
        write_raw_atomically(&raw_message_path(dir.path(), id.value()), b"raw")
            .await
            .unwrap();
    }

    let removed = empty_trash_impl(&store, &StubFactory, dir.path(), trash.value())
        .await
        .unwrap();

    assert_eq!(removed, 2);
    assert!(
        store
            .messages()
            .list_ids_in_mailbox(trash)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        read_raw(&raw_message_path(dir.path(), m1.value()))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn empty_trash_rejects_a_non_trash_folder() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = AccountId::new(
        create_account_impl(&store, &secrets, pop3_account_input(995))
            .await
            .unwrap()
            .id,
    );
    seed_standard_folders(&store, account).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let message = insert(&store, inbox).await;
    let dir = tempdir().unwrap();

    let result = empty_trash_impl(&store, &StubFactory, dir.path(), inbox.value()).await;
    assert!(result.is_err(), "only Trash may be emptied");
    // The inbox message is untouched.
    assert!(store.messages().get(message).await.unwrap().is_some());
}

#[tokio::test]
async fn compact_reclaims_only_orphaned_raw_blobs() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = AccountId::new(
        create_account_impl(&store, &secrets, valid_input())
            .await
            .unwrap()
            .id,
    );
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let dir = tempdir().unwrap();

    // One blob belongs to a live message; one is an orphan with no row.
    let kept = insert(&store, inbox).await;
    write_raw_atomically(&raw_message_path(dir.path(), kept.value()), b"live")
        .await
        .unwrap();
    write_raw_atomically(&raw_message_path(dir.path(), 999_999), b"orphan")
        .await
        .unwrap();

    let outcome = compact_storage_impl(&store, dir.path()).await.unwrap();

    assert_eq!(outcome.reclaimed, 1);
    assert!(
        read_raw(&raw_message_path(dir.path(), kept.value()))
            .await
            .unwrap()
            .is_some(),
        "a live message's body is kept"
    );
    assert!(
        read_raw(&raw_message_path(dir.path(), 999_999))
            .await
            .unwrap()
            .is_none(),
        "the orphan is reclaimed"
    );
}

#[tokio::test]
async fn empty_trash_on_exit_setting_round_trips_per_account() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    assert!(
        !empty_trash_on_exit_enabled(&store, account.id)
            .await
            .unwrap()
    );
    set_empty_trash_on_exit_enabled(&store, account.id, true)
        .await
        .unwrap();
    assert!(
        empty_trash_on_exit_enabled(&store, account.id)
            .await
            .unwrap()
    );
}
