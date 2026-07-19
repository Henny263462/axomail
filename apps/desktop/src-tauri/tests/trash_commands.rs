//! Integration tests for the Trash lifecycle commands. Empty Trash removes the
//! local rows and raw bodies (the server expunge is proven at the adapter
//! layer); Compact reclaims orphaned raw blobs; the per-account empty-on-exit
//! setting round-trips.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxRole, MessageFlags, MessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::trash::{
    compact_storage_impl, empty_trash_impl, reclaim_orphaned_blobs,
};
use axomail_desktop_lib::settings::{empty_trash_on_exit_enabled, set_empty_trash_on_exit_enabled};
use axomail_storage::{
    NewMessage, NewPendingOp, PendingOpKind, Store, raw_message_path, read_raw,
    write_raw_atomically,
};
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
async fn empty_trash_spares_a_row_with_an_unreplayed_relocate_op_and_its_blob() {
    // Findings 1 & 2: a message moved/deleted to Trash *optimistically* still
    // sits in its server source until its relocate op replays. Empty Trash must
    // neither delete that local row (the row-delete cascades away the pending op,
    // resurrecting the mail) nor reclaim its raw blob (which is still live). A
    // Trash message with no such op is purged, and only its blob is removed.
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    // POP3 so Empty Trash skips the server expunge; the purgeable logic is
    // protocol-agnostic (it keys off the pending op, not the account type).
    let account = AccountId::new(
        create_account_impl(&store, &secrets, pop3_account_input(995))
            .await
            .unwrap()
            .id,
    );
    seed_standard_folders(&store, account).await;
    let trash = mailbox_by_role(&store, account, MailboxRole::Trash).await;
    let dir = tempdir().unwrap();

    let purged = insert(&store, trash).await;
    let kept = insert(&store, trash).await; // carries an un-replayed delete op
    for id in [purged, kept] {
        write_raw_atomically(&raw_message_path(dir.path(), id.value()), b"raw")
            .await
            .unwrap();
    }
    // An un-replayed move-to-Trash op still queued for `kept`.
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: kept,
            kind: PendingOpKind::Delete,
            payload_json: "{}".to_owned(),
        })
        .await
        .unwrap();

    let removed = empty_trash_impl(&store, &StubFactory, dir.path(), trash.value())
        .await
        .unwrap();

    // Only the op-free message was purged.
    assert_eq!(removed, 1);
    assert!(
        store.messages().get(purged).await.unwrap().is_none(),
        "the fully-replayed Trash message is purged"
    );
    assert!(
        read_raw(&raw_message_path(dir.path(), purged.value()))
            .await
            .unwrap()
            .is_none(),
        "the purged message's blob is reclaimed"
    );
    // The op-bearing message and its op both survive, and its blob is kept.
    assert!(
        store.messages().get(kept).await.unwrap().is_some(),
        "the message with an un-replayed op survives Empty Trash"
    );
    assert_eq!(
        store.pending_ops().counts(account).await.unwrap().pending,
        1,
        "its pending relocate op is not cascaded away"
    );
    assert!(
        read_raw(&raw_message_path(dir.path(), kept.value()))
            .await
            .unwrap()
            .is_some(),
        "the surviving message keeps its still-live raw blob"
    );
}

#[tokio::test]
async fn compact_spares_a_live_blob_absent_from_a_stale_snapshot() {
    // Finding 3: the compact snapshot is taken once up front; a message inserted
    // and body-downloaded after it (its id therefore missing from the snapshot)
    // must not lose its valid blob. The live re-check spares it.
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

    // A live message whose blob is on disk, plus a true orphan with no row.
    let live = insert(&store, inbox).await;
    write_raw_atomically(&raw_message_path(dir.path(), live.value()), b"live")
        .await
        .unwrap();
    write_raw_atomically(&raw_message_path(dir.path(), 999_999), b"orphan")
        .await
        .unwrap();

    // A deliberately stale snapshot that omits `live` — as if it arrived after
    // the snapshot was taken.
    let stale: HashSet<i64> = HashSet::new();
    let reclaimed = reclaim_orphaned_blobs(&store, dir.path(), &stale)
        .await
        .unwrap();

    assert_eq!(reclaimed, 1, "only the true orphan is reclaimed");
    assert!(
        read_raw(&raw_message_path(dir.path(), live.value()))
            .await
            .unwrap()
            .is_some(),
        "the live message keeps its blob despite being absent from the snapshot"
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
