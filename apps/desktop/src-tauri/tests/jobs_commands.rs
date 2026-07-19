//! Integration tests for the activity-manager commands: `list_sync_jobs`
//! resolves each job's account/mailbox context and orders running → pending
//! → recent terminal (newest first); the cancel wrappers guard their id and
//! forward to the engine without error.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    Account, AccountId, IncomingMailProvider, MailError, MailboxId, MailboxName, MessageFlags,
    RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::jobs::{
    cancel_all_sync_jobs_impl, cancel_sync_job_impl, list_sync_jobs_impl,
};
use axomail_storage::{JobKind, JobState, NewSyncJob, Store, SyncJob};
use axomail_sync::{ProviderFactory, SyncConfig, SyncEngine};
use common::{secret_stores, valid_input};

/// A provider that never connects; the cancel tests never run a job, so its
/// bodies are only here to satisfy the trait.
#[derive(Default)]
struct StubProvider;

impl IncomingMailProvider for StubProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Err(MailError::Cancelled)
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
    async fn set_flags(
        &mut self,
        _id: &RemoteMessageId,
        _flags: MessageFlags,
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

/// Inserts a `pending` job for one mailbox and returns the created row.
async fn pending_job(store: &Store, account: i64, mailbox: i64) -> SyncJob {
    store
        .sync_jobs()
        .create_if_absent(&NewSyncJob {
            kind: JobKind::MailboxSync,
            account_id: Some(AccountId::new(account)),
            mailbox_id: Some(MailboxId::new(mailbox)),
        })
        .await
        .unwrap()
        .expect("a fresh mailbox has no active job")
}

#[tokio::test]
async fn list_sync_jobs_resolves_context_and_orders_by_state() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account = summary.id;
    // Four distinct mailboxes so no active job de-duplicates another.
    let inbox = summary.mailboxes[0].id;
    let drafts = summary.mailboxes[1].id;
    let sent = summary.mailboxes[2].id;
    let archive = summary.mailboxes[3].id;
    let mailbox_name = |id: i64| {
        summary
            .mailboxes
            .iter()
            .find(|mailbox| mailbox.id == id)
            .map(|mailbox| mailbox.name.clone())
            .unwrap()
    };

    // Running on the archive mailbox.
    let running = pending_job(&store, account, archive).await;
    store.sync_jobs().claim(running.id).await.unwrap();
    // Pending on the inbox mailbox.
    let pending = pending_job(&store, account, inbox).await;
    // Succeeded on the sent mailbox.
    let succeeded = pending_job(&store, account, sent).await;
    store
        .sync_jobs()
        .finish(succeeded.id, JobState::Succeeded, None)
        .await
        .unwrap();
    // Failed (with a redacted reason) on the drafts mailbox, created last so
    // it is the newest terminal row.
    let failed = pending_job(&store, account, drafts).await;
    store
        .sync_jobs()
        .finish(failed.id, JobState::Failed, Some("boom"))
        .await
        .unwrap();

    let jobs = list_sync_jobs_impl(&store).await.unwrap();
    assert_eq!(jobs.len(), 4, "every job is listed");

    // Order: running, then pending, then recent terminal newest-first.
    let ids: Vec<i64> = jobs.iter().map(|job| job.id).collect();
    assert_eq!(
        ids,
        vec![
            running.id.value(),
            pending.id.value(),
            failed.id.value(),
            succeeded.id.value()
        ],
        "running → pending → newest terminal first"
    );

    // The running job carries fully resolved display context.
    let first = &jobs[0];
    assert_eq!(first.state, "running");
    assert_eq!(first.kind, "mailboxSync");
    assert_eq!(first.account_id, Some(account));
    assert_eq!(first.account_label.as_deref(), Some("Work"));
    assert_eq!(first.mailbox_id, Some(archive));
    assert_eq!(
        first.mailbox_label.as_deref(),
        Some(mailbox_name(archive).as_str())
    );

    // The failed job keeps its state, redacted error, and finish time.
    let failed_dto = jobs.iter().find(|job| job.id == failed.id.value()).unwrap();
    assert_eq!(failed_dto.state, "failed");
    assert_eq!(failed_dto.error.as_deref(), Some("boom"));
    assert!(failed_dto.finished_at_unix.is_some());
}

#[tokio::test]
async fn list_sync_jobs_is_empty_when_no_jobs_exist() {
    let store = Store::open_in_memory().await.unwrap();
    let jobs = list_sync_jobs_impl(&store).await.unwrap();
    assert!(jobs.is_empty(), "a fresh store has no background jobs");
}

#[tokio::test]
async fn cancel_commands_guard_ids_and_forward_to_the_engine() {
    let store = Store::open_in_memory().await.unwrap();
    let engine = SyncEngine::start(store.clone(), StubFactory, SyncConfig::default());

    // Cancelling an unknown (but in-range) id is a harmless no-op.
    cancel_sync_job_impl(&engine, 4242).unwrap();

    // An out-of-range id cannot name any real job and is rejected.
    let error = cancel_sync_job_impl(&engine, i64::MAX).expect_err("an unsafe id must be rejected");
    assert!(matches!(error, MailError::Configuration { .. }));

    // Cancel-all is infallible and safe with nothing tracked.
    cancel_all_sync_jobs_impl(&engine);
}
