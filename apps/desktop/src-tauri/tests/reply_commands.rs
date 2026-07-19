//! Integration tests for the reply/forward prefill (`prepare_reply`) and the
//! post-send source marking (`mark_reply_source`), driven against an in-memory
//! store with a cached raw `.eml` fixture (no network).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MessageFlags, MessageId, RemoteMessageId,
    UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::reply::{mark_reply_source_impl, prepare_reply_impl};
use axomail_desktop_lib::dto::{ReplyMode, SourceMark};
use axomail_storage::{ForwardMode, NewMessage, PendingOpKind, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::provider::{StubFactory, stub_engine};
use common::{secret_stores, valid_input};

/// A plain-text source message addressed to the account (Klaus) plus another
/// recipient, with a Cc and a References chain — enough to exercise reply,
/// reply-all, and forward derivation.
const SOURCE: &str = concat!(
    "From: Ada Lovelace <ada@example.org>\r\n",
    "To: Klaus <klaus@theferbers.eu>, Bob <bob@example.org>\r\n",
    "Cc: Carol <carol@example.net>\r\n",
    "Subject: Weekly sync\r\n",
    "Date: Sun, 13 Sep 2020 12:26:40 +0000\r\n",
    "Message-ID: <parent-1@example.org>\r\n",
    "References: <root-0@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "\r\n",
    "Original line one.\r\nOriginal line two.\r\n",
);

/// Writes `bytes` to `<raw_root>/raw/<message_id>.eml`, creating the dir.
fn cache_raw(raw_root: &Path, message_id: i64, bytes: &str) {
    let dir = raw_root.join("raw");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{message_id}.eml")), bytes).unwrap();
}

/// Inserts a source-message row into `mailbox` with an optional remote identity.
async fn insert(store: &Store, mailbox: MailboxId, remote: Option<RemoteMessageId>) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: remote,
        subject: Some("Weekly sync".to_owned()),
        from: vec![Address {
            display_name: Some("Ada Lovelace".to_owned()),
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: Some(UtcTimestamp::from_unix_seconds(1_600_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: None,
        has_raw: true,
    };
    store.messages().insert(&message).await.unwrap().id
}

/// Creates an account (email klaus@theferbers.eu, one seeded inbox + default
/// identity), inserts the source message, and caches its raw `.eml`. Returns
/// the store, a temp raw root, the account id, and the source message id.
async fn setup() -> (Store, tempfile::TempDir, AccountId, i64) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account = AccountId::new(summary.id);
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = insert(&store, inbox, Some(imap_id(1))).await;
    cache_raw(raw_root.path(), message.value(), SOURCE);
    (store, raw_root, account, message.value())
}

fn imap_id(uid: u32) -> RemoteMessageId {
    RemoteMessageId::Imap {
        uid,
        uid_validity: 1,
    }
}

#[tokio::test]
async fn reply_prefills_sender_subject_threading_and_identity() {
    let (store, raw_root, account, message_id) = setup().await;
    let default_identity = store
        .identities()
        .list(account)
        .await
        .unwrap()
        .into_iter()
        .find(|identity| identity.is_default)
        .unwrap();

    let prefill = prepare_reply_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message_id,
        ReplyMode::Reply,
        false,
    )
    .await
    .unwrap();

    assert_eq!(prefill.to, vec!["Ada Lovelace <ada@example.org>"]);
    assert!(prefill.cc.is_empty());
    assert_eq!(prefill.subject, "Re: Weekly sync");
    assert_eq!(prefill.in_reply_to.as_deref(), Some("parent-1@example.org"));
    assert_eq!(
        prefill.references,
        vec!["root-0@example.org", "parent-1@example.org"]
    );
    assert!(prefill.body.contains("> Original line one."));
    assert!(prefill.attachments.is_empty());
    assert_eq!(prefill.source.message_id, message_id);
    assert_eq!(prefill.source.mark, SourceMark::Answered);
    // The account's own address is the delivered-to, so its identity is guessed.
    assert_eq!(prefill.identity_id, Some(default_identity.id.value()));
}

#[tokio::test]
async fn reply_all_gathers_other_recipients_and_excludes_self() {
    let (store, raw_root, _account, message_id) = setup().await;

    let prefill = prepare_reply_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message_id,
        ReplyMode::ReplyAll,
        false,
    )
    .await
    .unwrap();

    // Klaus (self) is dropped; Ada (sender) leads, then Bob; Carol lands in Cc.
    assert_eq!(
        prefill.to,
        vec!["Ada Lovelace <ada@example.org>", "Bob <bob@example.org>"]
    );
    assert_eq!(prefill.cc, vec!["Carol <carol@example.net>"]);
}

#[tokio::test]
async fn forward_inline_reproduces_the_original_without_recipients() {
    let (store, raw_root, _account, message_id) = setup().await;

    let prefill = prepare_reply_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message_id,
        ReplyMode::Forward,
        false,
    )
    .await
    .unwrap();

    assert!(prefill.to.is_empty());
    assert_eq!(prefill.subject, "Fwd: Weekly sync");
    assert!(prefill.in_reply_to.is_none());
    assert!(prefill.references.is_empty());
    assert!(prefill.body.contains("-------- Forwarded Message --------"));
    assert!(prefill.body.contains("Original line one."));
    assert!(
        prefill.attachments.is_empty(),
        "inline forward: no attachment"
    );
    assert_eq!(prefill.source.mark, SourceMark::Forwarded);
}

#[tokio::test]
async fn forward_as_attachment_carries_the_source_as_message_rfc822() {
    let (store, raw_root, account, message_id) = setup().await;
    // Switch the identity's forward mode to attachment.
    let identity = store
        .identities()
        .list(account)
        .await
        .unwrap()
        .into_iter()
        .find(|identity| identity.is_default)
        .unwrap();
    let mut fields = identity.fields.clone();
    fields.forward_mode = ForwardMode::Attachment;
    store
        .identities()
        .update(identity.id, &fields)
        .await
        .unwrap();

    let prefill = prepare_reply_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message_id,
        ReplyMode::Forward,
        false,
    )
    .await
    .unwrap();

    assert_eq!(prefill.attachments.len(), 1);
    let attachment = &prefill.attachments[0];
    assert_eq!(attachment.mime_type, "message/rfc822");
    assert!(attachment.filename.ends_with(".eml"));
    // The attachment is the verbatim source .eml.
    let decoded = STANDARD.decode(&attachment.content_base64).unwrap();
    assert_eq!(decoded, SOURCE.as_bytes());
    // The forwarded content is attached, not inlined in the body.
    assert!(!prefill.body.contains("-------- Forwarded Message --------"));
}

#[tokio::test]
async fn prepare_reply_offline_without_a_cached_body_is_an_error() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    // A remote-backed message whose body was never cached.
    let message = insert(&store, inbox, Some(imap_id(2))).await;

    let error = prepare_reply_impl(
        &store,
        &StubFactory,
        raw_root.path(),
        message.value(),
        ReplyMode::Reply,
        true,
    )
    .await
    .expect_err("offline reply without a cached body must fail");
    assert!(matches!(
        error,
        axomail_core::MailError::Configuration { .. }
    ));
}

#[tokio::test]
async fn mark_answered_sets_the_local_flag_and_enqueues_an_op() {
    let (store, _raw_root, account, message_id) = setup().await;
    let engine = stub_engine(&store);

    mark_reply_source_impl(&store, &engine, message_id, SourceMark::Answered)
        .await
        .unwrap();

    // Applied locally.
    assert!(
        store
            .messages()
            .get(MessageId::new(message_id))
            .await
            .unwrap()
            .unwrap()
            .flags
            .answered
    );
    // One add-flags op queued for propagation.
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::AddFlags);
}

#[tokio::test]
async fn mark_forwarded_enqueues_a_keyword_op_without_a_local_flag() {
    let (store, _raw_root, account, message_id) = setup().await;
    let engine = stub_engine(&store);

    mark_reply_source_impl(&store, &engine, message_id, SourceMark::Forwarded)
        .await
        .unwrap();

    // $Forwarded has no local column here, so the local answered flag is unset.
    assert!(
        !store
            .messages()
            .get(MessageId::new(message_id))
            .await
            .unwrap()
            .unwrap()
            .flags
            .answered
    );
    // A single add-keyword op is queued.
    let ops = store
        .pending_ops()
        .claim_next_batch(account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::AddKeyword);
    assert!(ops[0].payload_json.contains("$Forwarded"));
}
