//! Shared application state managed by the Tauri runtime.
//!
//! Holds the open [`Store`], the process-wide [`SecretStore`], the sync
//! engine and its provider factory, the background-sync [`SchedulerControl`],
//! and the on-disk root under which raw message bytes are cached. Every field
//! is cheap to clone (pools, `Arc`s, and a path), so command handlers can take
//! a `&AppState` and clone what they need onto worker tasks.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axomail_security::SecretStore;
use axomail_storage::Store;
use axomail_sync::{EngineOptions, SyncConfig, SyncEngine};

use crate::composition::{AnyIncomingProvider, AppProviderFactory};
use crate::scheduler::SchedulerControl;

/// A thread-safe, shareable secret store handle.
///
/// The secret store is used from blocking tasks (keychain I/O blocks), so it
/// must be `Send + Sync` and cheap to clone; an [`Arc`] over the trait object
/// satisfies both without committing command code to a concrete backend.
pub type SharedSecretStore = Arc<dyn SecretStore + Send + Sync>;

/// The concrete sync engine type wired for this application.
pub type AppSyncEngine = SyncEngine<AnyIncomingProvider, AppProviderFactory>;

/// State shared by every command handler.
///
/// Cloning is intentionally cheap and shares the underlying resources.
#[derive(Clone)]
pub struct AppState {
    store: Store,
    secrets: SharedSecretStore,
    factory: AppProviderFactory,
    engine: Arc<AppSyncEngine>,
    scheduler: Arc<SchedulerControl>,
    raw_root: PathBuf,
    /// Global Work Offline flag, shared with the sync engine (which gates job
    /// claims on it) and the scheduler. `set_work_offline` flips it through the
    /// engine so an online transition resumes queued work.
    offline: Arc<AtomicBool>,
}

impl AppState {
    /// Builds application state and starts the sync engine.
    ///
    /// Starting the engine spawns its dispatcher, which performs crash
    /// recovery once (requeue stale `running` jobs, resume `pending` ones)
    /// before serving enqueue requests. The background scheduler is created
    /// but not launched; call [`SchedulerControl::launch`] from the setup
    /// hook once the persisted interval is known.
    #[must_use]
    pub fn new(
        store: Store,
        secrets: SharedSecretStore,
        raw_root: PathBuf,
        initial_offline: bool,
    ) -> Self {
        let factory = AppProviderFactory::new(Arc::clone(&secrets), store.clone());
        // Seed the offline flag from the persisted setting before the engine's
        // dispatcher runs crash recovery, so a client that quit offline never
        // claims jobs on restart until the user goes back online.
        let offline = Arc::new(AtomicBool::new(initial_offline));
        // The engine caches raw bodies under `<raw_root>/raw/<id>.eml` for
        // offline reading, and shares the offline flag so job claims are gated.
        let engine = Arc::new(SyncEngine::start_with_options(
            store.clone(),
            factory.clone(),
            SyncConfig::default(),
            EngineOptions {
                offline: Arc::clone(&offline),
                raw_root: Some(Arc::from(raw_root.clone().into_boxed_path())),
            },
        ));
        let scheduler = Arc::new(SchedulerControl::new(
            Arc::clone(&engine),
            store.clone(),
            Arc::clone(&offline),
        ));
        Self {
            store,
            secrets,
            factory,
            engine,
            scheduler,
            raw_root,
            offline,
        }
    }

    /// Borrows the local database store.
    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Borrows the shared secret store handle.
    #[must_use]
    pub fn secrets(&self) -> &SharedSecretStore {
        &self.secrets
    }

    /// Borrows the provider factory (used for on-demand body fetches).
    #[must_use]
    pub fn factory(&self) -> &AppProviderFactory {
        &self.factory
    }

    /// Borrows the running sync engine.
    #[must_use]
    pub fn engine(&self) -> &AppSyncEngine {
        &self.engine
    }

    /// Borrows the background-sync scheduler control.
    #[must_use]
    pub fn scheduler(&self) -> &Arc<SchedulerControl> {
        &self.scheduler
    }

    /// Root directory under which raw message bytes are cached
    /// (`<raw_root>/raw/<message_id>.eml`).
    #[must_use]
    pub fn raw_root(&self) -> &Path {
        &self.raw_root
    }

    /// Whether the app is currently working offline (reads local, queues
    /// writes/sends). Shared with the sync engine and scheduler.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }
}
