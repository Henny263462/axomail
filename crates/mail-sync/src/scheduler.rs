//! Background synchronization scheduling.
//!
//! [`BackgroundScheduler`] runs alongside the [`SyncEngine`](crate::SyncEngine)
//! and periodically enqueues [`SyncMailbox`] jobs for a configurable set of
//! `(account, mailbox)` targets. It exists so mailboxes stay fresh without a
//! user pressing refresh, while deliberately never polling aggressively.
//!
//! # Behaviour
//!
//! - The first run is delayed ([`SchedulerConfig::first_delay`], default 30s)
//!   so application start stays snappy.
//! - Thereafter each account is polled every [`SchedulerConfig::interval`]
//!   (default 10 min), clamped up to a hard floor of one minute — short-interval
//!   polling is a non-goal.
//! - Enqueuing relies on the engine's de-duplication: an equivalent
//!   pending/running job is skipped, so a busy engine never accumulates a
//!   backlog of scheduler-created work.
//! - Offline backoff is per account: a job that fails with
//!   [`SyncErrorClass::Temporary`](crate::SyncErrorClass) doubles that
//!   account's interval (capped at 60 min) until a success resets it. The
//!   scheduler learns of failures and successes purely from the engine's
//!   broadcast events.
//!
//! The scheduler owns exactly one background task, driven by a
//! [`tokio::select`] over a shutdown token, a wake signal, the engine event
//! stream, and a single timer. It never busy-waits and leaves no task behind
//! once [`BackgroundScheduler::shutdown`] returns or the handle is dropped.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{AccountId, IncomingMailProvider, MailError, MailboxId};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::engine::SyncEngine;
use crate::event::SyncEvent;
use crate::factory::ProviderFactory;
use crate::job::{SyncMailbox, WorkRequest};

mod task;
#[cfg(test)]
mod tests;

/// The engine surface the scheduler drives: enqueue work and observe progress.
///
/// Introduced so the background task depends on a minimal interface rather than
/// the concrete engine. The only production implementor is [`SyncEngine`]; the
/// crate's own tests substitute a lightweight, storage-free fake. This trait is
/// internal — it does not widen the public API.
pub(crate) trait JobSink: Send + Sync + 'static {
    /// Enqueues a mailbox for synchronization (equivalent pending/running work
    /// is de-duplicated by the implementor).
    fn enqueue(&self, job: SyncMailbox) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Enqueues an account's write-back op replay as a periodic backstop, so
    /// queued offline ops drain even when no interactive command or crash
    /// recovery has triggered a replay (equivalent work is de-duplicated by the
    /// implementor).
    fn enqueue_op_replay(
        &self,
        account: AccountId,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Subscribes to the progress event stream.
    fn subscribe(&self) -> broadcast::Receiver<SyncEvent>;
}

impl<P, F> JobSink for SyncEngine<P, F>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    fn enqueue(&self, job: SyncMailbox) -> impl Future<Output = Result<(), MailError>> + Send {
        SyncEngine::enqueue(self, job)
    }

    fn enqueue_op_replay(
        &self,
        account: AccountId,
    ) -> impl Future<Output = Result<(), MailError>> + Send {
        SyncEngine::enqueue_work(
            self,
            WorkRequest::OpReplay {
                account_id: account,
            },
        )
    }

    fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        SyncEngine::subscribe(self)
    }
}

/// Hard floor on the polling interval: the scheduler never polls more often
/// than this regardless of configuration.
const MIN_INTERVAL: Duration = Duration::from_secs(60);

/// Upper bound the per-account offline backoff interval grows to.
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// Tuning parameters for a [`BackgroundScheduler`].
///
/// [`SchedulerConfig::default`] provides production-sensible values; the
/// command layer overrides them as needed.
#[derive(Debug, Clone, Copy)]
pub struct SchedulerConfig {
    /// Base polling interval per account. Values below the one-minute floor
    /// are raised to it; offline backoff extends it further, per account.
    pub interval: Duration,
    /// Delay before the very first poll after the scheduler starts, keeping
    /// application startup responsive.
    pub first_delay: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(600),
            first_delay: Duration::from_secs(30),
        }
    }
}

impl SchedulerConfig {
    /// The base interval after the one-minute floor is applied.
    fn base_interval(self) -> Duration {
        self.interval.max(MIN_INTERVAL)
    }
}

/// A serializable snapshot of the scheduler, suitable for a status command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerStatus {
    /// `true` while the scheduler is actively enqueuing; `false` when paused.
    pub running: bool,
    /// The enforced base polling interval, in seconds.
    pub interval_secs: u64,
    /// Accounts currently in offline backoff, mapped to their present
    /// (extended) interval in seconds. Accounts polling at the base interval
    /// are absent.
    pub per_account_backoff: BTreeMap<AccountId, u64>,
}

/// State shared between the public handle and the background task.
struct SchedulerState {
    /// Whether enqueuing is currently suspended.
    paused: bool,
    /// Target mailboxes grouped by owning account.
    targets: BTreeMap<AccountId, BTreeSet<MailboxId>>,
    /// Current backoff interval per account; only present while an account is
    /// backed off (interval above the base).
    backoff: BTreeMap<AccountId, Duration>,
}

/// A handle to a running background scheduler.
///
/// Dropping the handle (or calling [`BackgroundScheduler::shutdown`]) stops
/// the background task; no timers or tasks are left running afterwards.
pub struct BackgroundScheduler {
    state: Arc<Mutex<SchedulerState>>,
    wake: Arc<Notify>,
    shutdown: CancellationToken,
    task: Option<JoinHandle<()>>,
    base_interval: Duration,
}

impl BackgroundScheduler {
    /// Starts a scheduler that enqueues into `engine` for the given `targets`.
    ///
    /// The returned handle is protocol-agnostic: the generic engine is moved
    /// into the background task, so callers hold a plain, non-generic handle.
    #[must_use]
    pub fn start<P, F>(
        engine: Arc<SyncEngine<P, F>>,
        config: SchedulerConfig,
        targets: impl IntoIterator<Item = SyncMailbox>,
    ) -> Self
    where
        P: IncomingMailProvider + 'static,
        F: ProviderFactory<P>,
    {
        Self::start_with_sink(engine, config, targets)
    }

    /// Starts a scheduler over any [`JobSink`]; the public [`Self::start`] is
    /// the production entry point (the sink is the engine).
    pub(crate) fn start_with_sink<S: JobSink>(
        sink: Arc<S>,
        config: SchedulerConfig,
        targets: impl IntoIterator<Item = SyncMailbox>,
    ) -> Self {
        let state = Arc::new(Mutex::new(SchedulerState {
            paused: false,
            targets: group_targets(targets),
            backoff: BTreeMap::new(),
        }));
        let wake = Arc::new(Notify::new());
        let shutdown = CancellationToken::new();
        let base_interval = config.base_interval();
        let task = tokio::spawn(task::run(
            sink,
            Arc::clone(&state),
            Arc::clone(&wake),
            shutdown.clone(),
            config,
        ));
        Self {
            state,
            wake,
            shutdown,
            task: Some(task),
            base_interval,
        }
    }

    /// Suspends enqueuing. In-flight jobs already handed to the engine are
    /// unaffected; no new jobs are scheduled until [`Self::resume`].
    pub fn pause(&self) {
        lock(&self.state).paused = true;
        self.wake.notify_one();
    }

    /// Resumes enqueuing after a [`Self::pause`]. Targets whose interval has
    /// already elapsed are polled promptly (and de-duplicated by the engine).
    pub fn resume(&self) {
        lock(&self.state).paused = false;
        self.wake.notify_one();
    }

    /// Replaces the set of scheduled targets.
    ///
    /// Backoff state for accounts that remain targeted is preserved; state for
    /// accounts that are dropped is discarded.
    pub fn update_targets(&self, targets: impl IntoIterator<Item = SyncMailbox>) {
        let next = group_targets(targets);
        {
            let mut state = lock(&self.state);
            state
                .backoff
                .retain(|account, _| next.contains_key(account));
            state.targets = next;
        }
        self.wake.notify_one();
    }

    /// Returns a serializable snapshot of the current scheduler state.
    #[must_use]
    pub fn status(&self) -> SchedulerStatus {
        let state = lock(&self.state);
        SchedulerStatus {
            running: !state.paused,
            interval_secs: self.base_interval.as_secs(),
            per_account_backoff: state
                .backoff
                .iter()
                .map(|(account, interval)| (*account, interval.as_secs()))
                .collect(),
        }
    }

    /// Stops the scheduler and waits for its background task to finish.
    ///
    /// After this returns, the scheduler holds no timers and its task has
    /// exited, releasing its reference to the engine.
    pub async fn shutdown(mut self) {
        self.shutdown.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for BackgroundScheduler {
    fn drop(&mut self) {
        // Cancel first so a still-running task observes the shutdown and exits
        // on its own; abort is a safety net for a task parked elsewhere.
        self.shutdown.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Groups a flat target list into mailboxes per account.
fn group_targets(
    targets: impl IntoIterator<Item = SyncMailbox>,
) -> BTreeMap<AccountId, BTreeSet<MailboxId>> {
    let mut grouped: BTreeMap<AccountId, BTreeSet<MailboxId>> = BTreeMap::new();
    for target in targets {
        grouped
            .entry(target.account_id)
            .or_default()
            .insert(target.mailbox_id);
    }
    grouped
}

/// Locks a [`Mutex`] without panicking on poison: the guarded state is plain
/// data, so recovering the inner value after a prior panic is safe.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
