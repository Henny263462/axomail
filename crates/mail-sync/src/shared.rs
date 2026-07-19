//! Runtime state shared between the dispatcher and every worker task.
//!
//! Held behind an `Arc` so the dispatcher, all workers, and the public
//! engine handle observe the same channels, semaphore, per-account locks,
//! and cancellation tokens.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axomail_core::AccountId;
use axomail_storage::{Store, SyncJobId};
use tokio::sync::{Mutex as AsyncMutex, Notify, Semaphore, broadcast};
use tokio_util::sync::CancellationToken;

use crate::config::SyncConfig;
use crate::event::SyncEvent;
use crate::factory::ProviderFactory;

/// State every part of the engine shares.
///
/// `P`/`F` are the provider and its factory; only [`Self::factory`] holds
/// `F`, while `P` is a phantom marker threaded through the worker's
/// generic functions.
pub(crate) struct Shared<P, F> {
    /// Local storage handle.
    pub(crate) store: Store,
    /// Provider factory supplied by the composition root.
    pub(crate) factory: Arc<F>,
    /// Tuning parameters.
    pub(crate) config: SyncConfig,
    /// Root under which raw message bodies are cached
    /// (`<raw_root>/raw/<id>.eml`), when the composition root enabled the
    /// offline body store. `None` disables sync-time body download.
    pub(crate) raw_root: Option<Arc<Path>>,
    /// Global "work offline" gate. When set, workers claim no jobs; going
    /// back online notifies [`Self::resume`] to re-spawn pending work.
    offline: Arc<AtomicBool>,
    /// Pulsed when the engine returns online, so the dispatcher resumes any
    /// jobs that stayed pending while offline.
    pub(crate) resume: Arc<Notify>,
    /// Progress event fan-out.
    events: broadcast::Sender<SyncEvent>,
    /// Global parallelism bound.
    pub(crate) semaphore: Arc<Semaphore>,
    /// One async mutex per account, enforcing that an account never runs
    /// two jobs at once.
    account_locks: Mutex<HashMap<AccountId, Arc<AsyncMutex<()>>>>,
    /// Cancellation token per in-flight job.
    cancels: Mutex<HashMap<SyncJobId, CancellationToken>>,
    _provider: PhantomData<fn() -> P>,
}

impl<P, F> Shared<P, F>
where
    F: ProviderFactory<P>,
    P: axomail_core::IncomingMailProvider,
{
    /// Builds the shared state and the broadcast sender.
    pub(crate) fn new(
        store: Store,
        factory: F,
        config: SyncConfig,
        offline: Arc<AtomicBool>,
        raw_root: Option<Arc<Path>>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(config.event_buffer.max(1));
        Arc::new(Self {
            store,
            factory: Arc::new(factory),
            config,
            raw_root,
            offline,
            resume: Arc::new(Notify::new()),
            events,
            semaphore: Arc::new(Semaphore::new(config.max_parallel_jobs.max(1))),
            account_locks: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            _provider: PhantomData,
        })
    }

    /// Whether the engine is currently working offline (no new claims).
    pub(crate) fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }

    /// Sets the offline flag, returning the previous value so the caller can
    /// detect an online transition.
    pub(crate) fn set_offline(&self, offline: bool) -> bool {
        self.offline.swap(offline, Ordering::SeqCst)
    }

    /// Subscribes a new observer to progress events.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.events.subscribe()
    }

    /// Broadcasts an event, ignoring the "no current subscribers" case.
    pub(crate) fn emit(&self, event: SyncEvent) {
        let _ = self.events.send(event);
    }

    /// Returns the async mutex serializing work for `account`, creating
    /// it on first use.
    pub(crate) fn account_lock(&self, account: AccountId) -> Arc<AsyncMutex<()>> {
        let mut map = lock(&self.account_locks);
        map.entry(account)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Registers a fresh cancellation token for `job` and returns a clone.
    pub(crate) fn register_cancel(&self, job: SyncJobId) -> CancellationToken {
        let token = CancellationToken::new();
        lock(&self.cancels).insert(job, token.clone());
        token
    }

    /// Removes and returns the cancellation token for `job`, if any.
    pub(crate) fn take_cancel(&self, job: SyncJobId) {
        lock(&self.cancels).remove(&job);
    }

    /// Cancels the token for a single job, if it is registered.
    pub(crate) fn cancel(&self, job: SyncJobId) {
        if let Some(token) = lock(&self.cancels).get(&job) {
            token.cancel();
        }
    }

    /// Cancels every registered job.
    pub(crate) fn cancel_all(&self) {
        for token in lock(&self.cancels).values() {
            token.cancel();
        }
    }
}

/// Locks a [`Mutex`] without panicking on poison: a poisoned lock means a
/// prior holder panicked, but the maps hold only handles and cloning them
/// after recovery is safe.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
