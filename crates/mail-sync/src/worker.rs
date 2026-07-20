//! Per-job execution: the shared claim/serialize/finalize shape plus the
//! per-kind executor dispatch.
//!
//! Every job — whatever its kind — runs the same durable shape: take the
//! account lock, acquire a global parallelism permit, atomically claim the
//! pending row, announce `JobStarted`, run the kind's executor, then record a
//! terminal state and emit the closing event. [`dispatch`] routes the claimed
//! job to its executor: `MailboxSync` keeps its full incremental pipeline (in
//! [`mailbox_sync`]); `Discover` reconciles the remote folder tree (in
//! [`discover`]); `OpReplay` drains the write-back queue (in [`op_replay`]).

mod discover;
mod filter_arrival;
mod junk_route;
mod mailbox_sync;
mod offline_download;
mod op_replay;

use std::sync::Arc;

use axomail_core::{AccountId, IncomingMailProvider, MailError};
use axomail_storage::{JobKind, JobState, NewSyncJob, SyncJob};
use tokio_util::sync::CancellationToken;

use crate::event::{SyncEvent, SyncJobSummary};
use crate::factory::ProviderFactory;
use crate::shared::Shared;

/// Runs a claimed-or-pending job to a terminal state, then deregisters its
/// cancellation token.
pub(crate) async fn run_job<P, F>(shared: Arc<Shared<P, F>>, job: SyncJob, token: CancellationToken)
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    // Work Offline gate: never claim a job while offline. The row stays
    // `pending` and is re-spawned when the engine returns online (see
    // `SyncEngine::set_offline`). Checked before any lock so an offline engine
    // does no work and holds no resources.
    if shared.is_offline() {
        shared.take_cancel(job.id);
        return;
    }

    // Every kind is account-scoped; the mailbox (if any) is resolved by the
    // per-kind executor. A row without an account is a corruption, not work.
    let Some(account_id) = job.account_id else {
        finalize(
            &shared,
            &job,
            Err(MailError::Configuration {
                message: "sync job is missing its account".to_owned(),
            }),
        )
        .await;
        shared.take_cancel(job.id);
        return;
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

    // Claim the job under the account lock so no two `running` rows for one
    // account can ever coexist. A non-pending row means someone else already
    // handled it; leave its terminal state untouched.
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
        dispatch(&shared, &job, account_id, &token).await
    };

    let succeeded = result.is_ok();
    finalize(&shared, &job, result).await;
    // Close the drain-completion window: an op enqueued after this OpReplay's
    // final empty claim but before its terminal transition was deduped away (an
    // OpReplay was still `running`, so the command path's enqueue was a no-op).
    // Now that the job is terminal, a fresh OpReplay can drain it. Only on
    // success — a failed drain is retried by the scheduler backstop, never
    // re-spawned in a tight loop.
    if succeeded && job.kind == JobKind::OpReplay {
        redrain_pending_ops(&shared, account_id).await;
    }
    shared.take_cancel(job.id);
}

/// After an `OpReplay` job finishes, enqueues a successor when the account still
/// has claimable pending ops (see [`run_job`]). The successor is created with
/// `create_if_absent` (deduped against any op-replay already queued or running)
/// and the dispatcher is woken via `resume` to spawn it. Claimability excludes
/// ops blocked behind a parked head, so this never spins.
async fn redrain_pending_ops<P, F>(shared: &Arc<Shared<P, F>>, account_id: AccountId)
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    match shared
        .store
        .pending_ops()
        .account_has_claimable_ops(account_id)
        .await
    {
        Ok(true) => {
            let new = NewSyncJob {
                kind: JobKind::OpReplay,
                account_id: Some(account_id),
                mailbox_id: None,
            };
            match shared.store.sync_jobs().create_if_absent(&new).await {
                // A fresh op-replay row: wake the dispatcher to spawn it.
                Ok(Some(_)) => shared.resume.notify_one(),
                // An op-replay is already queued/running; it will cover them.
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(error = %error, "failed to enqueue successor op-replay");
                }
            }
        }
        Ok(false) => {}
        Err(error) => {
            tracing::error!(error = %error, "failed to re-check pending ops after replay");
        }
    }
}

/// Routes a claimed job to its per-kind executor.
///
/// The account lock, permit, and claim are already held/done by [`run_job`],
/// so each executor only implements the kind-specific work. `MailboxSync`,
/// `Discover`, and `OpReplay` each have a real executor; `Fetch`/`Send` are not
/// engine-driven kinds.
async fn dispatch<P, F>(
    shared: &Arc<Shared<P, F>>,
    job: &SyncJob,
    account_id: AccountId,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    match job.kind {
        JobKind::MailboxSync => mailbox_sync::execute(shared, job, account_id, token).await,
        JobKind::Discover => discover::execute(shared, account_id, token).await,
        JobKind::OpReplay => op_replay::execute(shared, job, account_id, token).await,
        JobKind::Fetch | JobKind::Send => Err(MailError::NotImplemented {
            feature: format!("job kind {} is not engine-driven", job.kind.as_str()),
        }),
    }
}

/// Awaits a fallible network future under both a per-operation timeout and the
/// job's cancellation token. Cancellation wins ties. Shared by every executor.
pub(super) async fn guarded<T, Fut>(
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
            // Cooperative cancellation is a deliberate user action, not an
            // error: record the cancelled state and announce it with a
            // distinct terminal event so the frontend never renders it as a
            // failure.
            let message = MailError::Cancelled.to_string();
            record(shared, job, JobState::Cancelled, Some(&message)).await;
            shared.emit(SyncEvent::JobCancelled { job_id: job.id });
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
