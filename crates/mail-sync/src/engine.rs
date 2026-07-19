//! The public [`SyncEngine`] handle plus its dispatcher task.
//!
//! Enqueue requests arrive over a bounded channel; a single dispatcher
//! task creates (and de-duplicates) the persistent job row, announces it,
//! and spawns a worker. On startup the dispatcher first requeues jobs a
//! crash left `running` and resumes everything still `pending`.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use axomail_core::{AccountId, IncomingMailProvider, MailError, MailboxId};
use axomail_storage::{JobKind, JobState, NewSyncJob, Store, SyncJob, SyncJobId};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::SyncConfig;
use crate::event::SyncEvent;
use crate::factory::ProviderFactory;
use crate::job::{SyncMailbox, WorkRequest};
use crate::shared::Shared;
use crate::worker::run_job;

/// Optional wiring the composition root supplies to [`SyncEngine::start_with_options`].
///
/// Both fields default to "off": no shared offline flag and no raw-body store,
/// which is exactly what the plain [`SyncEngine::start`] and the crate's own
/// tests want. The desktop app opts in to both so mailbox sync can pre-download
/// bodies for offline reading and honour a global Work Offline toggle.
#[derive(Clone, Default)]
pub struct EngineOptions {
    /// Shared "work offline" flag; when set, workers claim no jobs. Shared with
    /// the app so the status-bar toggle and the scheduler observe one source of
    /// truth. Defaults to a private, always-online flag.
    pub offline: Arc<AtomicBool>,
    /// Root under which raw bodies are cached (`<raw_root>/raw/<id>.eml`).
    /// `None` disables sync-time offline body download.
    pub raw_root: Option<Arc<Path>>,
}

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
    commands: mpsc::Sender<WorkRequest>,
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
        Self::start_with_options(store, factory, config, EngineOptions::default())
    }

    /// Starts the engine with explicit [`EngineOptions`] (offline flag and raw
    /// body store). [`Self::start`] is this with defaults.
    #[must_use]
    pub fn start_with_options(
        store: Store,
        factory: F,
        config: SyncConfig,
        options: EngineOptions,
    ) -> Self {
        let shared = Shared::new(store, factory, config, options.offline, options.raw_root);
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
        self.enqueue_work(WorkRequest::MailboxSync(job)).await
    }

    /// Enqueues any [`WorkRequest`] (mailbox sync, discover, or op replay).
    ///
    /// The dispatcher persists it as a `sync_jobs` row, de-duplicates
    /// equivalent pending/running work per the request's dedup key, and
    /// routes the claimed job to its per-kind executor. This is the general
    /// entry point the Wave 2 tracks use to enqueue `Discover`/`OpReplay`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Cancelled`] if the engine has shut down and no
    /// longer accepts work.
    pub async fn enqueue_work(&self, request: WorkRequest) -> Result<(), MailError> {
        self.commands
            .send(request)
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

    /// Sets the engine's Work Offline state.
    ///
    /// While offline the engine claims no jobs: a spawned worker returns
    /// without touching its pending row (see the claim-time gate in
    /// [`crate::worker::run_job`]). Returning online re-spawns every job that
    /// stayed pending, so queued work resumes automatically. Cheap and
    /// idempotent; safe to call from a command handler.
    pub fn set_offline(&self, offline: bool) {
        let was_offline = self.shared.set_offline(offline);
        if was_offline && !offline {
            // Online transition: wake the dispatcher to resume pending jobs.
            self.shared.resume.notify_one();
        }
    }

    /// Whether the engine is currently working offline.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.shared.is_offline()
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
    mut commands: mpsc::Receiver<WorkRequest>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
) where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if let Err(error) = recover(&shared, &workers).await {
        tracing::error!(error = %error, "sync engine startup recovery failed");
    }

    loop {
        tokio::select! {
            biased;
            // An online transition wakes us to resume anything left pending
            // while offline. Checked first so a burst of enqueues cannot
            // starve resumption.
            () = shared.resume.notified() => {
                if let Err(error) = resume_pending(&shared, &workers).await {
                    tracing::error!(error = %error, "failed to resume pending jobs after going online");
                }
            }
            request = commands.recv() => match request {
                Some(request) => {
                    if let Err(error) = enqueue_job(&shared, &workers, request).await {
                        tracing::error!(error = %error, "failed to enqueue sync job");
                    }
                }
                // The engine handle was dropped: stop serving.
                None => break,
            },
        }
    }
}

/// Re-spawns a worker for every job still `pending`, used when the engine
/// returns online. Workers that were spawned while offline have already exited
/// at the claim-time gate, so their rows have no live worker; a fresh worker
/// claims each (double-spawns are harmless — the atomic claim lets only one
/// win). Never requeues `running` rows.
async fn resume_pending<P, F>(
    shared: &Arc<Shared<P, F>>,
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let pending = shared
        .store
        .sync_jobs()
        .list_by_state(JobState::Pending)
        .await?;
    for job in pending {
        spawn_worker(shared, workers, job);
    }
    Ok(())
}

/// Requeues crash-orphaned `running` jobs and resumes every `pending` one,
/// regardless of kind (the worker routes each to its per-kind executor).
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

    // Write-back ops a crash left `inflight` would otherwise block their
    // message chains forever (an inflight head is never re-claimed). Recover
    // them here, once, before any worker runs — the only safe moment, since a
    // running drain owns its account's inflight ops.
    let requeued_ops = shared.store.pending_ops().requeue_stale().await?;
    if requeued_ops > 0 {
        tracing::info!(
            count = requeued_ops,
            "requeued stale pending ops after restart"
        );
    }

    // Ops enqueued before a crash (or otherwise left with no live replay job)
    // are drained by enqueuing one `OpReplay` per account that still has
    // claimable work. Without this, `requeue_stale` alone would leave those ops
    // sitting `pending` forever, since `OpReplay` was only ever enqueued from
    // the interactive command path. Created before the pending-job scan so the
    // same spawn loop starts them; `create_if_absent` dedups against any
    // op-replay a stale `running` row already contributed.
    for account_id in shared
        .store
        .pending_ops()
        .accounts_with_claimable_ops()
        .await?
    {
        let new = NewSyncJob {
            kind: JobKind::OpReplay,
            account_id: Some(account_id),
            mailbox_id: None,
        };
        shared.store.sync_jobs().create_if_absent(&new).await?;
    }

    let pending = shared
        .store
        .sync_jobs()
        .list_by_state(JobState::Pending)
        .await?;
    for job in pending {
        spawn_worker(shared, workers, job);
    }
    Ok(())
}

/// Creates (de-duplicating) the job row, announces it, and starts it.
async fn enqueue_job<P, F>(
    shared: &Arc<Shared<P, F>>,
    workers: &Arc<Mutex<Vec<JoinHandle<()>>>>,
    request: WorkRequest,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let (kind, account_id, mailbox_id) = describe(request);
    let new = NewSyncJob {
        kind,
        account_id: Some(account_id),
        mailbox_id,
    };
    match shared.store.sync_jobs().create_if_absent(&new).await? {
        Some(job) => {
            // Announce before the worker can emit `JobStarted`, so events
            // for a job always begin with `JobQueued`.
            shared.emit(SyncEvent::JobQueued {
                job_id: job.id,
                account_id,
                mailbox_id,
            });
            spawn_worker(shared, workers, job);
        }
        None => {
            tracing::debug!(
                kind = kind.as_str(),
                account_id = %account_id,
                "skipped duplicate sync job"
            );
        }
    }
    Ok(())
}

/// Maps a [`WorkRequest`] to the persisted job's `(kind, account, mailbox)`.
/// Account-scoped kinds carry no mailbox.
fn describe(request: WorkRequest) -> (JobKind, AccountId, Option<MailboxId>) {
    match request {
        WorkRequest::MailboxSync(mailbox) => (
            JobKind::MailboxSync,
            mailbox.account_id,
            Some(mailbox.mailbox_id),
        ),
        WorkRequest::Discover { account_id } => (JobKind::Discover, account_id, None),
        WorkRequest::OpReplay { account_id } => (JobKind::OpReplay, account_id, None),
    }
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
