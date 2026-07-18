//! Shared application state managed by the Tauri runtime.
//!
//! Holds the open [`Store`] and the process-wide [`SecretStore`]. Both are
//! cheap to share: the store wraps a reference-counted connection pool and
//! the secret store is held behind an [`Arc`] so command handlers can clone
//! a handle onto a blocking task without borrowing across an `await`.

use std::sync::Arc;

use axomail_security::SecretStore;
use axomail_storage::Store;

/// A thread-safe, shareable secret store handle.
///
/// The secret store is used from blocking tasks (keychain I/O blocks), so it
/// must be `Send + Sync` and cheap to clone; an [`Arc`] over the trait object
/// satisfies both without committing command code to a concrete backend.
pub type SharedSecretStore = Arc<dyn SecretStore + Send + Sync>;

/// State shared by every command handler.
///
/// Cloning is intentionally cheap and shares the underlying resources, so
/// handlers may take a `&AppState` from [`tauri::State`] and clone what they
/// need onto worker tasks.
#[derive(Clone)]
pub struct AppState {
    store: Store,
    secrets: SharedSecretStore,
}

impl AppState {
    /// Builds application state from an open store and a secret store.
    #[must_use]
    pub fn new(store: Store, secrets: SharedSecretStore) -> Self {
        Self { store, secrets }
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
}
