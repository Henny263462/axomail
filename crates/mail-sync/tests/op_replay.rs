//! Tests for the keyword op-replay executor: an `add_keyword` pending op turns
//! into an additive `store_flags` write on the server; other op kinds and
//! local-only messages are handled without a network write.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex};

use axomail_core::{
    Address, EmailAddress, FlagDelta, IncomingMailProvider, Keyword, MailError, MailboxId,
    MailboxName, MessageFlags, MessageId, RawMessage, RemoteMailboxState, RemoteMessageId,
    SyncOutcome, SyncRequest,
};
use axomail_storage::{KeywordOpPayload, NewMessage, NewPendingOp, PendingOpKind, Store};
use axomail_sync::{KeywordReplayOutcome, replay_keyword_op};

use support::{batch, envelope, store_with_mailbox};

/// A provider that records every `store_flags` call and no-ops everything else.
#[derive(Default)]
struct RecordingProvider {
    stores: Arc<Mutex<Vec<(RemoteMessageId, FlagDelta)>>>,
}

impl IncomingMailProvider for RecordingProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Ok(RemoteMailboxState {
            mailboxes: Vec::new(),
        })
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        Err(MailError::Cancelled)
    }
    async fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> Result<(), MailError> {
        self.stores
            .lock()
            .unwrap()
            .push((id.clone(), delta.clone()));
        Ok(())
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
    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

/// A local-only message (no remote identity) to insert directly.
fn local_message(mailbox: MailboxId) -> NewMessage {
    NewMessage {
        mailbox,
        remote_id: None,
        subject: Some("draft".to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        date: None,
        flags: MessageFlags::default(),
        size_bytes: None,
        preview: None,
        has_raw: false,
    }
}

/// Ingests one IMAP message and returns its local id.
async fn imap_message(store: &Store, mailbox: MailboxId) -> MessageId {
    store
        .ingest()
        .apply(
            mailbox,
            &batch(100, 1, false, vec![envelope(1, 100, "hi", false)]),
        )
        .await
        .unwrap();
    store
        .messages()
        .list_page(mailbox, None, 10)
        .await
        .unwrap()
        .items
        .first()
        .unwrap()
        .id
}

async fn enqueue_keyword_op(
    store: &Store,
    account: axomail_core::AccountId,
    message: MessageId,
    kind: PendingOpKind,
    keyword: &str,
) -> axomail_storage::PendingOp {
    let payload = KeywordOpPayload {
        keyword: Keyword::parse(keyword).unwrap(),
    }
    .to_json()
    .unwrap();
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: message,
            kind,
            payload_json: payload,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn add_keyword_op_issues_additive_store() {
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = imap_message(&store, mailbox).await;
    let op = enqueue_keyword_op(&store, account, message, PendingOpKind::AddKeyword, "$Junk").await;

    let mut provider = RecordingProvider::default();
    let recorded = Arc::clone(&provider.stores);
    let outcome = replay_keyword_op(&store, &mut provider, &op).await.unwrap();

    assert_eq!(outcome, KeywordReplayOutcome::Applied);
    let calls = recorded.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let (id, delta) = &calls[0];
    assert_eq!(
        *id,
        RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 100
        }
    );
    // Additive: the keyword is on the add side, nothing on remove.
    assert!(delta.remove.is_empty());
    let added: Vec<&str> = delta.add.keywords.iter().map(Keyword::as_str).collect();
    assert_eq!(added, vec!["$Junk"]);
}

#[tokio::test]
async fn remove_keyword_op_issues_additive_remove() {
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = imap_message(&store, mailbox).await;
    let op = enqueue_keyword_op(
        &store,
        account,
        message,
        PendingOpKind::RemoveKeyword,
        "$Junk",
    )
    .await;

    let mut provider = RecordingProvider::default();
    let recorded = Arc::clone(&provider.stores);
    replay_keyword_op(&store, &mut provider, &op).await.unwrap();

    let calls = recorded.lock().unwrap();
    let (_, delta) = &calls[0];
    assert!(delta.add.is_empty());
    let removed: Vec<&str> = delta.remove.keywords.iter().map(Keyword::as_str).collect();
    assert_eq!(removed, vec!["$Junk"]);
}

#[tokio::test]
async fn non_keyword_op_is_skipped_without_a_write() {
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = imap_message(&store, mailbox).await;
    let op = store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: message,
            kind: PendingOpKind::AddFlags,
            payload_json: "{}".to_owned(),
        })
        .await
        .unwrap();

    let mut provider = RecordingProvider::default();
    let recorded = Arc::clone(&provider.stores);
    let outcome = replay_keyword_op(&store, &mut provider, &op).await.unwrap();

    assert_eq!(outcome, KeywordReplayOutcome::NotKeywordOp);
    assert!(recorded.lock().unwrap().is_empty(), "no server write");
}

#[tokio::test]
async fn local_only_message_has_no_remote_target() {
    let (store, account, mailbox) = store_with_mailbox().await;
    // A local message (no remote id) has no server keyword target.
    let message = store
        .messages()
        .insert(&local_message(mailbox))
        .await
        .unwrap()
        .id;
    let op = enqueue_keyword_op(&store, account, message, PendingOpKind::AddKeyword, "$Junk").await;

    let mut provider = RecordingProvider::default();
    let recorded = Arc::clone(&provider.stores);
    let outcome = replay_keyword_op(&store, &mut provider, &op).await.unwrap();

    assert_eq!(outcome, KeywordReplayOutcome::NoRemoteTarget);
    assert!(recorded.lock().unwrap().is_empty());
}
