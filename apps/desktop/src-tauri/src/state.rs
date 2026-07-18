//! Shared application state managed by the Tauri runtime.
//!
//! Holds the open [`Store`], the process-wide [`SecretStore`], the sync
//! engine and its provider factory, and the on-disk root under which raw
//! message bytes are cached. Every field is cheap to clone (pools, `Arc`s,
//! and a path), so command handlers can take a `&AppState` and clone what
//! they need onto worker tasks.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axomail_security::SecretStore;
use axomail_storage::Store;
use axomail_sync::{SyncConfig, SyncEngine};

use crate::composition::{AnyIncomingProvider, AppProviderFactory};

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
    raw_root: PathBuf,
}

impl AppState {
    /// Builds application state and starts the sync engine.
    ///
    /// Starting the engine spawns its dispatcher, which performs crash
    /// recovery once (requeue stale `running` jobs, resume `pending` ones)
    /// before serving enqueue requests.
    #[must_use]
    pub fn new(store: Store, secrets: SharedSecretStore, raw_root: PathBuf) -> Self {
        let factory = AppProviderFactory::new(Arc::clone(&secrets));
        let engine = Arc::new(SyncEngine::start(
            store.clone(),
            factory.clone(),
            SyncConfig::default(),
        ));
        Self {
            store,
            secrets,
            factory,
            engine,
            raw_root,
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

    /// Root directory under which raw message bytes are cached
    /// (`<raw_root>/raw/<message_id>.eml`).
    #[must_use]
    pub fn raw_root(&self) -> &Path {
        &self.raw_root
    }
}
