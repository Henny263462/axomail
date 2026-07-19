//! Integration tests for the message-action commands (move / copy / archive /
//! delete-to-Trash / undo): each applies to local storage first and, for a
//! write-back-capable IMAP message, enqueues the matching `pending_ops` row.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxRole, MessageFlags, MessageId,
    RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::actions::{
    archive_messages_impl, copy_messages_impl, delete_to_trash_impl, move_messages_impl,
    undo_move_impl,
};
use axomail_storage::{NewMessage, PendingOpKind, Store};
use common::provider::stub_engine;
use common::{secret_stores, seed_standard_folders, valid_input};
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

/// Inserts a message with (or without) a remote identity into `mailbox`.
async fn insert(store: &Store, mailbox: MailboxId, remote: Option<RemoteMessageId>) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: remote,
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
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id
}

fn imap_id(uid: u32) -> RemoteMessageId {
    RemoteMessageId::Imap {
        uid,
        uid_validity: 1,
    }
}

/// Creates an IMAP account with the full standard folder set and returns its id.
async fn account_with_folders(store: &Store) -> AccountId {
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(store, &secrets, valid_input())
        .await
        .unwrap();
    let id = AccountId::new(account.id);
    seed_standard_folders(store, id).await;
    id
}

#[tokio::test]
async fn move_applies_locally_and_enqueues_a_move_op_with_an_undo_record() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);

    let outcome = move_messages_impl(&store, &engine, vec![message.value()], archive.value())
        .await
        .unwrap();

    // Optimistic local move.
    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        archive
    );
    // The undo record points back to the source inbox.
    assert_eq!(outcome.moves.len(), 1);
    assert_eq!(outcome.moves[0].message_id, message.value());
    assert_eq!(outcome.moves[0].from_mailbox_id, inbox.value());
    // A move op is queued for the server.
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::Move);
}

#[tokio::test]
async fn local_only_move_enqueues_no_op() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    // No remote identity → nothing to propagate.
    let message = insert(&store, inbox, None).await;
    let engine = stub_engine(&store);

    move_messages_impl(&store, &engine, vec![message.value()], archive.value())
        .await
        .unwrap();

    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        archive
    );
    assert_eq!(
        store.pending_ops().counts(account).await.unwrap().pending,
        0
    );
}

#[tokio::test]
async fn archive_files_into_the_archive_role_folder() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);

    archive_messages_impl(&store, &engine, vec![message.value()])
        .await
        .unwrap();

    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        archive
    );
}

#[tokio::test]
async fn archive_without_an_archive_folder_errors_before_moving() {
    let store = Store::open_in_memory().await.unwrap();
    // Only the inbox is seeded — no Archive folder.
    let (_c, secrets) = secret_stores();
    let account = AccountId::new(
        create_account_impl(&store, &secrets, valid_input())
            .await
            .unwrap()
            .id,
    );
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);

    let result = archive_messages_impl(&store, &engine, vec![message.value()]).await;
    assert!(result.is_err(), "archive must error with no Archive folder");
    // Pre-flight error left the message in place.
    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        inbox
    );
}

#[tokio::test]
async fn delete_moves_to_trash_and_enqueues_a_delete_op() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let trash = mailbox_by_role(&store, account, MailboxRole::Trash).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);

    delete_to_trash_impl(&store, &engine, vec![message.value()])
        .await
        .unwrap();

    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        trash
    );
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::Delete);
}

#[tokio::test]
async fn undo_moves_a_message_back_to_its_source() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);

    let outcome = move_messages_impl(&store, &engine, vec![message.value()], archive.value())
        .await
        .unwrap();
    undo_move_impl(&store, &engine, outcome.moves.clone())
        .await
        .unwrap();

    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        inbox
    );
}

#[tokio::test]
async fn copy_imap_enqueues_a_copy_op_and_keeps_the_source() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    let engine = stub_engine(&store);
    let dir = tempdir().unwrap();

    copy_messages_impl(
        &store,
        &engine,
        dir.path(),
        vec![message.value()],
        archive.value(),
    )
    .await
    .unwrap();

    // The source stays put; no local duplicate (sync ingests the server copy).
    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        inbox
    );
    assert!(
        store
            .messages()
            .list_ids_in_mailbox(archive)
            .await
            .unwrap()
            .is_empty()
    );
    // A copy op rides the Move kind with `copy: true`.
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::Move);
    assert!(ops[0].payload_json.contains("\"copy\":true"));
}

#[tokio::test]
async fn copy_local_duplicates_the_row_into_the_target() {
    let store = Store::open_in_memory().await.unwrap();
    let account = account_with_folders(&store).await;
    let inbox = mailbox_by_role(&store, account, MailboxRole::Inbox).await;
    let archive = mailbox_by_role(&store, account, MailboxRole::Archive).await;
    // Local-only message → duplicated as a new local row.
    let message = insert(&store, inbox, None).await;
    let engine = stub_engine(&store);
    let dir = tempdir().unwrap();

    copy_messages_impl(
        &store,
        &engine,
        dir.path(),
        vec![message.value()],
        archive.value(),
    )
    .await
    .unwrap();

    // Source kept; a duplicate now sits in the target; nothing queued.
    assert_eq!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .mailbox,
        inbox
    );
    assert_eq!(
        store
            .messages()
            .list_ids_in_mailbox(archive)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store.pending_ops().counts(account).await.unwrap().pending,
        0
    );
}
