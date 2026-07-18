//! Per-job execution: claiming, per-account serialization, the pass loop
//! with incremental ingestion, transient-failure retries, and terminal
//! state recording.

use std::sync::Arc;

use axomail_core::{
    Account, ErrorClass, IncomingMailProvider, MailError, MailboxId, MailboxName, SyncRequest,
};
use axomail_storage::{JobState, SyncJob};
use tokio_util::sync::CancellationToken;

use crate::backoff::backoff_delay;
use crate::event::{SyncEvent, SyncJobSummary};
use crate::factory::ProviderFactory;
use crate::shared::Shared;

/// Runs a claimed-or-pending job to a terminal state, then deregisters
/// its cancellation token.
pub(crate) async fn run_job<P, F>(shared: Arc<Shared<P, F>>, job: SyncJob, token: CancellationToken)
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let (account_id, mailbox_id) = match (job.account_id, job.mailbox_id) {
        (Some(account), Some(mailbox)) => (account, mailbox),
        _ => {
            finalize(
                &shared,
                &job,
                Err(MailError::Configuration {
                    message: "sync job is missing its account or mailbox".to_owned(),
                }),
            )
            .await;
            shared.take_cancel(job.id);
            return;
        }
    };

    // Serialize per account first, then take a global parallelism permit.
    // Holding the account lock while awaiting a permit only ever blocks
    // other jobs of the *same* account, which is exactly the invariant.
    let account_lock = shared.account_lock(account_id);
    let _account_guard = account_lock.lock_owned().await;
    let _permit = match Arc::clone(&shared.semaphore).acquire_owned().await {
        Ok(permit) => permit,
        // The semaphore is only ever closed while shutting down.
        Err(_) => {
            shared.take_cancel(job.id);
            return;
        }
    };

    // Claim the job under the account lock so no two `running` rows for
    // one account can ever coexist. A non-pending row means someone else
    // already handled it; leave its terminal state untouched.
    match shared.store.sync_jobs().claim(job.id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            shared.take_cancel(job.id);
            return;
        }
        Err(error) => {
            finalize(&shared, &job, Err(error)).await;
            shared.take_cancel(job.id);
            return;
        }
    }

    shared.emit(SyncEvent::JobStarted { job_id: job.id });

    let result = if token.is_cancelled() {
        Err(MailError::Cancelled)
    } else {
        execute(&shared, job.id, account_id, mailbox_id, &token).await
    };

    finalize(&shared, &job, result).await;
    shared.take_cancel(job.id);
}

/// Resolves account and mailbox context, then drives the retry loop.
async fn execute<P, F>(
    shared: &Arc<Shared<P, F>>,
    job_id: axomail_storage::SyncJobId,
    account_id: axomail_core::AccountId,
    mailbox_id: MailboxId,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = shared
        .store
        .accounts()
        .get(account_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "account for sync job no longer exists".to_owned(),
        })?;
    let remote_name = resolve_mailbox_name(shared, account_id, mailbox_id).await?;
    run_with_retry(shared, job_id, mailbox_id, &remote_name, &account, token).await
}

/// Finds the remote name of `mailbox_id` within its account.
async fn resolve_mailbox_name<P, F>(
    shared: &Arc<Shared<P, F>>,
    account_id: axomail_core::AccountId,
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
    job_id: axomail_storage::SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
    account: &Account,
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
        match run_passes(shared, job_id, mailbox_id, remote_name, account, token).await {
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
    job_id: axomail_storage::SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
    account: &Account,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    let mut provider = guarded(token, net, shared.factory.make(account)).await?;
    let outcome = pump(
        shared,
        job_id,
        mailbox_id,
        remote_name,
        &mut provider,
        token,
    )
    .await;
    // Close the session regardless of how the pass loop ended.
    let _ = guarded(token, net, provider.disconnect()).await;
    outcome
}

/// Repeatedly requests and ingests sync passes until `has_more` clears,
/// persisting each pass so progress survives a crash.
async fn pump<P, F>(
    shared: &Arc<Shared<P, F>>,
    job_id: axomail_storage::SyncJobId,
    mailbox_id: MailboxId,
    remote_name: &MailboxName,
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

/// Awaits a fallible network future under both a per-operation timeout
/// and the job's cancellation token. Cancellation wins ties.
async fn guarded<T, Fut>(
    token: &CancellationToken,
    timeout: std::time::Duration,
    future: Fut,
) -> Result<T, MailError>
where
    Fut: Future<Output = Result<T, MailError>>,
{
    tokio::select! {
        biased;
        () = token.cancelled() => Err(MailError::Cancelled),
        result = tokio::time::timeout(timeout, future) => match result {
            Ok(inner) => inner,
            Err(_elapsed) => Err(MailError::Timeout { after: timeout }),
        },
    }
}

/// Records the terminal outcome in storage and emits the closing event.
async fn finalize<P, F>(
    shared: &Arc<Shared<P, F>>,
    job: &SyncJob,
    result: Result<SyncJobSummary, MailError>,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    match result {
        Ok(summary) => {
            record(shared, job, JobState::Succeeded, None).await;
            shared.emit(SyncEvent::JobFinished {
                job_id: job.id,
                summary,
            });
        }
        Err(MailError::Cancelled) => {
            let message = MailError::Cancelled.to_string();
            record(shared, job, JobState::Cancelled, Some(&message)).await;
            shared.emit(SyncEvent::JobFailed {
                job_id: job.id,
                class: ErrorClass::Permanent.into(),
                message,
            });
        }
        Err(error) => {
            // `MailError` messages are redacted by construction (ADR-0003),
            // so the rendered string is safe to persist and broadcast.
            let class = error.class();
            let message = error.to_string();
            record(shared, job, JobState::Failed, Some(&message)).await;
            shared.emit(SyncEvent::JobFailed {
                job_id: job.id,
                class: class.into(),
                message,
            });
        }
    }
}

/// Persists a job's terminal state, logging (never surfacing) a storage
/// failure so event delivery still happens.
async fn record<P, F>(
    shared: &Arc<Shared<P, F>>,
    job: &SyncJob,
    state: JobState,
    error: Option<&str>,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if let Err(storage_error) = shared.store.sync_jobs().finish(job.id, state, error).await {
        tracing::error!(
            job_id = %job.id,
            error = %storage_error,
            "failed to record terminal sync-job state"
        );
    }
}
