//! The `MailboxSync` executor: resolve context, then the retry loop with
//! incremental, commit-then-continue pass ingestion.
//!
//! Behaviour is unchanged from the pre-generalization engine — this is the
//! same pipeline, moved behind the per-kind [`dispatch`](super::dispatch)
//! seam. Every network round-trip goes through [`super::guarded`] for uniform
//! timeout and cancellation handling.

use std::sync::Arc;

use axomail_core::{
    Account, AccountId, ErrorClass, IncomingMailProvider, MailError, MailboxId, MailboxName,
    SyncRequest,
};
use axomail_storage::{SyncJob, SyncJobId};
use tokio_util::sync::CancellationToken;

use crate::backoff::backoff_delay;
use crate::event::{SyncEvent, SyncJobSummary};
use crate::factory::ProviderFactory;
use crate::shared::Shared;

use super::guarded;

/// Resolves the job's mailbox and account, then drives the retry loop.
pub(super) async fn execute<P, F>(
    shared: &Arc<Shared<P, F>>,
    job: &SyncJob,
    account_id: AccountId,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox_id = job.mailbox_id.ok_or_else(|| MailError::Configuration {
        message: "mailbox-sync job is missing its mailbox".to_owned(),
    })?;
    let account = shared
        .store
        .accounts()
        .get(account_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "account for sync job no longer exists".to_owned(),
        })?;
    let remote_name = resolve_mailbox_name(shared, account_id, mailbox_id).await?;
    // Decide the QRESYNC present-set safety-net cadence once per job, so a
    // reconcile pass is consistent across this job's retries and passes.
    let full_reconcile = shared.take_full_reconcile(mailbox_id);
    let summary = run_with_retry(
        shared,
        job.id,
        mailbox_id,
        &remote_name,
        &account,
        full_reconcile,
        token,
    )
    .await?;

    // Run-on-arrival filters: applied after a successful inbox sync. A failure
    // here never fails the sync — it is logged and the pass continues.
    if let Err(error) = super::filter_arrival::run(shared, account_id, mailbox_id).await {
        tracing::warn!(error = %error, "run-on-arrival filter pass failed");
    }
    Ok(summary)
}

/// Finds the remote name of `mailbox_id` within its account.
async fn resolve_mailbox_name<P, F>(
    shared: &Arc<Shared<P, F>>,
    account_id: AccountId,
    mailbox_id: MailboxId,
) -> Result<MailboxName, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    shared
        .store
        .mailboxes()
        .list(account_id)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.id == mailbox_id)
        .map(|mailbox| mailbox.name)
        .ok_or_else(|| MailError::Configuration {
            message: "mailbox for sync job no longer exists".to_owned(),
        })
}

/// Retries the pass loop on transient failures with backoff, up to the
/// configured attempt budget; permanent, configuration, and cancellation
/// errors fail immediately.
async fn run_with_retry<P, F>(
    shared: &Arc<Shared<P, F>>,
    job_id: SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
    account: &Account,
    full_reconcile: bool,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut transient_failures = 0u32;
    loop {
        if token.is_cancelled() {
            return Err(MailError::Cancelled);
        }
        match run_passes(
            shared,
            job_id,
            mailbox_id,
            remote_name,
            account,
            full_reconcile,
            token,
        )
        .await
        {
            Ok(summary) => return Ok(summary),
            Err(error) => {
                if error.class() != ErrorClass::Temporary {
                    return Err(error);
                }
                transient_failures += 1;
                if transient_failures >= shared.config.max_attempts {
                    return Err(error);
                }
                let delay = backoff_delay(transient_failures, &shared.config);
                tokio::select! {
                    biased;
                    () = token.cancelled() => return Err(MailError::Cancelled),
                    () = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

/// One full connection lifecycle: build a provider, connect, pump batches
/// until the server reports no more, and disconnect (best effort).
async fn run_passes<P, F>(
    shared: &Arc<Shared<P, F>>,
    job_id: SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
    account: &Account,
    full_reconcile: bool,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    let mut provider = guarded(token, net, shared.factory.make(account)).await?;
    let mut outcome = pump(
        shared,
        job_id,
        mailbox_id,
        remote_name,
        full_reconcile,
        &mut provider,
        token,
    )
    .await;
    // With the envelope sync applied and the connection still open, download
    // the policy-covered raw bodies for offline reading (bounded per pass).
    // A body-download hiccup must not fail an otherwise successful sync, but a
    // cancellation still propagates.
    if let Ok(summary) = &mut outcome {
        match super::offline_download::run(shared, mailbox_id, &mut provider, token).await {
            Ok(cached) => summary.bodies_cached += cached,
            Err(MailError::Cancelled) => outcome = Err(MailError::Cancelled),
            Err(error) => {
                tracing::warn!(error = %error, "offline body download pass failed");
            }
        }
    }
    // Close the session regardless of how the pass loop ended.
    let _ = guarded(token, net, provider.disconnect()).await;
    outcome
}

/// Repeatedly requests and ingests sync passes until `has_more` clears,
/// persisting each pass so progress survives a crash.
async fn pump<P, F>(
    shared: &Arc<Shared<P, F>>,
    job_id: SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
    full_reconcile: bool,
    provider: &mut P,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    guarded(token, net, provider.connect()).await?;

    let mut summary = SyncJobSummary::default();
    loop {
        if token.is_cancelled() {
            return Err(MailError::Cancelled);
        }
        let state = shared.store.sync_state().get(mailbox_id).await?;
        let request = SyncRequest {
            mailbox: mailbox_id,
            remote_name: remote_name.clone(),
            state,
            batch_size: shared.config.batch_size,
            full_reconcile,
        };
        let outcome = guarded(token, net, provider.sync(request)).await?;
        let ingest = shared.store.ingest().apply(mailbox_id, &outcome).await?;
        summary.absorb(&ingest);
        shared.emit(SyncEvent::BatchIngested {
            job_id,
            mailbox_id,
            new_messages: ingest.inserted,
        });
        if !outcome.has_more {
            break;
        }
    }
    Ok(summary)
}
