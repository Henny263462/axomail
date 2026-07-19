//! Integration tests for the write-back commands: they apply a flag change to
//! local storage first and, for a write-back-capable IMAP account with a
//! server-backed message, enqueue a `pending_ops` row. POP3 and local-only
//! messages record locally without ever queuing an op.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    Account, AccountId, Address, EmailAddress, FlagDelta, IncomingMailProvider, MailError,
    MailboxId, MailboxName, MailboxRole, MessageFlags, MessageId, RawMessage, RemoteMailboxState,
    RemoteMessageId, SyncOutcome, SyncRequest, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::writeback::{
    mark_mailbox_read_impl, mark_message_seen_impl, mark_messages_read_impl,
    set_message_flagged_impl,
};
use axomail_storage::{NewMessage, PendingOpKind, Store};
use axomail_sync::{ProviderFactory, SyncConfig, SyncEngine};
use common::{pop3_account_input, secret_stores, valid_input};

/// A provider that never connects; the write-back commands only enqueue and
/// trigger a replay, so the replay job simply fails fast and never touches the
/// freshly enqueued ops.
#[derive(Default)]
struct StubProvider;

impl IncomingMailProvider for StubProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Err(MailError::Configuration {
            message: "stub never connects".to_owned(),
        })
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Err(MailError::Cancelled)
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        Err(MailError::Cancelled)
    }
    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn store_flags(
        &mut self,
        _id: &RemoteMessageId,
        _delta: &FlagDelta,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn move_message(
        &mut self,
        _id: &RemoteMessageId,
        _target: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn create_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn rename_mailbox(
        &mut self,
        _from: &MailboxName,
        _to: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn set_mailbox_subscribed(
        &mut self,
        _path: &MailboxName,
        _subscribed: bool,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

struct StubFactory;

impl ProviderFactory<StubProvider> for StubFactory {
    async fn make(&self, _account: &Account) -> Result<StubProvider, MailError> {
        Ok(StubProvider)
    }
}

fn engine(store: &Store) -> SyncEngine<StubProvider, StubFactory> {
    SyncEngine::start(store.clone(), StubFactory, SyncConfig::default())
}

/// The inbox mailbox id of a freshly created account.
async fn inbox(store: &Store, account: AccountId) -> MailboxId {
    store
        .mailboxes()
        .list(account)
        .await
        .unwrap()
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .expect("seeded inbox")
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

#[tokio::test]
async fn mark_seen_applies_locally_and_enqueues_an_imap_op() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let message = insert(&store, mailbox, Some(imap_id(1))).await;
    let engine = engine(&store);

    mark_message_seen_impl(&store, &engine, message.value(), true)
        .await
        .unwrap();

    // Applied locally.
    assert!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .flags
            .seen
    );
    // Enqueued exactly one op for propagation.
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account.id))
        .await
        .unwrap();
    assert_eq!(counts.pending, 1, "one add-flags op is queued");
}

#[tokio::test]
async fn marking_already_seen_is_a_no_op_and_queues_nothing() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let message = insert(&store, mailbox, Some(imap_id(1))).await;
    let engine = engine(&store);

    // Already unseen -> marking unseen changes nothing and queues nothing.
    mark_message_seen_impl(&store, &engine, message.value(), false)
        .await
        .unwrap();
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account.id))
        .await
        .unwrap();
    assert_eq!(counts.pending, 0);
}

#[tokio::test]
async fn star_toggle_enqueues_a_flag_op() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let message = insert(&store, mailbox, Some(imap_id(1))).await;
    let engine = engine(&store);

    set_message_flagged_impl(&store, &engine, message.value(), true)
        .await
        .unwrap();

    assert!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .flags
            .flagged
    );
    let ops = store
        .pending_ops()
        .claim_next_batch(AccountId::new(account.id), 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::AddFlags);
}

#[tokio::test]
async fn pop3_flag_change_records_locally_without_queuing() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    // POP3 accounts have no server-side flags, so nothing must be queued.
    let account = create_account_impl(&store, &secrets, pop3_account_input(9950))
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let message = insert(
        &store,
        mailbox,
        Some(RemoteMessageId::Pop3 {
            uidl: "uidl-1".to_owned(),
        }),
    )
    .await;
    let engine = engine(&store);

    mark_message_seen_impl(&store, &engine, message.value(), true)
        .await
        .unwrap();

    assert!(
        store
            .messages()
            .get(message)
            .await
            .unwrap()
            .unwrap()
            .flags
            .seen,
        "local flag still applied"
    );
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account.id))
        .await
        .unwrap();
    assert_eq!(counts.pending, 0, "POP3 never queues a server op");
}

#[tokio::test]
async fn mark_mailbox_read_bulk_marks_and_queues_per_message() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let a = insert(&store, mailbox, Some(imap_id(1))).await;
    let b = insert(&store, mailbox, Some(imap_id(2))).await;
    // A third message is already read: it must not be re-marked or queued.
    let read = insert(&store, mailbox, Some(imap_id(3))).await;
    store
        .messages()
        .update_flags(
            read,
            &MessageFlags {
                seen: true,
                ..MessageFlags::default()
            },
        )
        .await
        .unwrap();
    let engine = engine(&store);

    let marked = mark_mailbox_read_impl(&store, &engine, mailbox.value())
        .await
        .unwrap();

    assert_eq!(marked, 2, "only the two unread messages were marked");
    for id in [a, b] {
        assert!(store.messages().get(id).await.unwrap().unwrap().flags.seen);
    }
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account.id))
        .await
        .unwrap();
    assert_eq!(counts.pending, 2, "one op per newly-read message");
}

#[tokio::test]
async fn mark_messages_read_marks_a_selection_and_queues_per_message() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let mailbox = inbox(&store, AccountId::new(account.id)).await;
    let a = insert(&store, mailbox, Some(imap_id(1))).await;
    let b = insert(&store, mailbox, Some(imap_id(2))).await;
    // A third message is left out of the selection: it must stay unread.
    let untouched = insert(&store, mailbox, Some(imap_id(3))).await;
    let engine = engine(&store);

    mark_messages_read_impl(&store, &engine, vec![a.value(), b.value()], true)
        .await
        .unwrap();

    for id in [a, b] {
        assert!(store.messages().get(id).await.unwrap().unwrap().flags.seen);
    }
    assert!(
        !store
            .messages()
            .get(untouched)
            .await
            .unwrap()
            .unwrap()
            .flags
            .seen,
        "a message outside the selection is left unread"
    );
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account.id))
        .await
        .unwrap();
    assert_eq!(counts.pending, 2, "one op per message in the selection");
}
