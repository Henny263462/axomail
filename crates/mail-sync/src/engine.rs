//! The public [`SyncEngine`] handle plus its dispatcher task.
//!
//! Enqueue requests arrive over a bounded channel; a single dispatcher
//! task creates (and de-duplicates) the persistent job row, announces it,
//! and spawns a worker. On startup the dispatcher first requeues jobs a
//! crash left `running` and resumes everything still `pending`.

use std::sync::{Arc, Mutex};

use axomail_core::{IncomingMailProvider, MailError};
use axomail_storage::{JobKind, JobState, NewSyncJob, Store, SyncJob, SyncJobId};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::SyncConfig;
use crate::event::SyncEvent;
use crate::factory::ProviderFactory;
use crate::job::SyncMailbox;
use crate::shared::Shared;
use crate::worker::run_job;

/// A running synchronization engine.
///
/// Cloning is intentionally not offered: the engine owns its background
/// tasks and aborts them when dropped (an abrupt stop that models a
/// crash — persisted `running` rows are recovered by the next engine's
/// startup requeue). Share it behind an `Arc` if several owners need it.
pub struct SyncEngine<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    shared: Arc<Shared<P, F>>,
    commands: mpsc::Sender<SyncMailbox>,
    dispatcher: JoinHandle<()>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl<P, F> SyncEngine<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    /// Starts the engine over `store`, building providers with `factory`.
    ///
    /// Spawns the dispatcher, which immediately performs crash recovery
    /// (requeue stale `running` jobs, resume `pending` ones) before
    /// serving enqueue requests.
    #[must_use]
    pub fn start(store: Store, factory: F, config: SyncConfig) -> Self {
        let shared = Shared::new(store, factory, config);
        let (commands, command_rx) = mpsc::channel(config.command_buffer.max(1));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let dispatcher = tokio::spawn(dispatch(
            Arc::clone(&shared),
            command_rx,
            Arc::clone(&workers),
        ));
        Self {
            shared,
            commands,
            dispatcher,
            workers,
        }
    }

    /// Subscribes to progress events.
    ///
    /// A subscriber that falls behind the broadcast buffer observes a
    /// lag error for the missed events but keeps receiving new ones.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.shared.subscribe()
    }

    /// Enqueues a mailbox for synchronization.
    ///
    /// Applies backpressure when the bounded command channel is full.
    /// Duplicate work is skipped later, at row-creation time.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Cancelled`] if the engine has shut down and
    /// no longer accepts work.
    pub async fn enqueue(&self, job: SyncMailbox) -> Result<(), MailError> {
        self.commands
            .send(job)
            .await
            .map_err(|_| MailError::Cancelled)
    }

    /// Requests cancellation of a single in-flight or queued job.
    ///
    /// Cancellation is cooperative: the job stops at its next checkpoint
    /// and is recorded as cancelled.
    pub fn cancel(&self, job_id: SyncJobId) {
        self.shared.cancel(job_id);
    }

    /// Requests cancellation of every currently tracked job.
    pub fn cancel_all(&self) {
        self.shared.cancel_all();
    }
}

impl<P, F> Drop for SyncEngine<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    fn drop(&mut self) {
        // Abrupt stop: abort the dispatcher and any in-flight workers.
        // Uncommitted ingest transactions roll back and the jobs stay
        // `running` in storage until the next engine requeues them.
        self.dispatcher.abort();
        if let Ok(workers) = self.workers.lock() {
            for handle in workers.iter() {
                handle.abort();
            }
        }
    }
}

/// The dispatcher task: recover, then serve enqueue commands until the
/// engine is dropped and the channel closes.
async fn dispatch<P, F>(
    shared: Arc<Shared<P, F>>,
    mut commands: mpsc::Receiver<SyncMailbox>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if let Err(error) = recover(&shared, &workers).await {
        tracing::error!(error = %error, "sync engine startup recovery failed");
    }

    while let Some(job) = commands.recv().await {
        if let Err(error) = enqueue_job(&shared, &workers, job).await {
            tracing::error!(error = %error, "failed to enqueue sync job");
        }
    }
}

/// Requeues crash-orphaned `running` jobs and resumes `pending` ones.
async fn recover<P, F>(
    shared: &Arc<Shared<P, F>>,
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let requeued = shared.store.sync_jobs().requeue_stale().await?;
    if requeued > 0 {
        tracing::info!(count = requeued, "requeued stale sync jobs after restart");
    }

    let pending = shared
        .store
        .sync_jobs()
        .list_by_state(JobState::Pending)
        .await?;
    for job in pending {
        if job.kind == JobKind::MailboxSync {
            spawn_worker(shared, workers, job);
        }
    }
    Ok(())
}

/// Creates (de-duplicating) the job row, announces it, and starts it.
async fn enqueue_job<P, F>(
    shared: &Arc<Shared<P, F>>,
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
    request: SyncMailbox,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let new = NewSyncJob {
        kind: JobKind::MailboxSync,
        account_id: Some(request.account_id),
        mailbox_id: Some(request.mailbox_id),
    };
    match shared.store.sync_jobs().create_if_absent(&new).await? {
        Some(job) => {
            // Announce before the worker can emit `JobStarted`, so events
            // for a job always begin with `JobQueued`.
            shared.emit(SyncEvent::JobQueued {
                job_id: job.id,
                account_id: request.account_id,
                mailbox_id: request.mailbox_id,
            });
            spawn_worker(shared, workers, job);
        }
        None => {
            tracing::debug!(
                account_id = %request.account_id,
                mailbox_id = %request.mailbox_id,
                "skipped duplicate sync job"
            );
        }
    }
    Ok(())
}

/// Registers a cancellation token (before any event that reveals the id)
/// and spawns the worker, pruning finished handles as it goes.
fn spawn_worker<P, F>(
    shared: &Arc<Shared<P, F>>,
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
    job: SyncJob,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let token = shared.register_cancel(job.id);
    let handle = tokio::spawn(run_job(Arc::clone(shared), job, token));
    if let Ok(mut guard) = workers.lock() {
        guard.retain(|existing| !existing.is_finished());
        guard.push(handle);
    }
}
