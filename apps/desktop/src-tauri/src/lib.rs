//! axomail desktop application shell.
//!
//! Hosts the Tauri runtime and exposes the command surface to the Vue
//! frontend. All mail logic lives in the `crates/` workspace members; this
//! crate stays a thin composition layer that wires storage, the keychain,
//! the protocol adapters, and the sync engine together (ADR-0004).

pub mod commands;
pub mod composition;
pub mod dto;
pub mod state;

use std::sync::Arc;

use axomail_security::KeyringSecretStore;
use axomail_storage::Store;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

use crate::dto::SyncEventDto;
use crate::state::{AppState, SharedSecretStore};

/// Name of the SQLite database file within the application data directory.
const DATABASE_FILE: &str = "axomail.db";

/// Tauri event channel carrying [`SyncEventDto`] progress to the frontend.
const SYNC_EVENT: &str = "axomail://sync";

/// Builds and runs the Tauri application.
///
/// The setup hook resolves the application data directory, opens the local
/// store (running migrations), starts the sync engine (whose dispatcher
/// performs crash recovery once), and forwards the engine's progress events
/// to the frontend as [`SYNC_EVENT`]. If storage cannot be opened the
/// process exits non-zero without presenting a window: the client is useless
/// without its local database.
pub fn run() {
    let outcome = tauri::Builder::default()
        .setup(|app| {
            let data_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(error) => {
                    eprintln!("fatal: cannot resolve the application data directory: {error}");
                    std::process::exit(1);
                }
            };
            let database_path = data_dir.join(DATABASE_FILE);
            let store = match tauri::async_runtime::block_on(Store::open(&database_path)) {
                Ok(store) => store,
                Err(error) => {
                    eprintln!("fatal: cannot open the local database: {error}");
                    std::process::exit(1);
                }
            };
            let secrets: SharedSecretStore = Arc::new(KeyringSecretStore::new());
            // Build inside the async runtime: starting the engine spawns Tokio
            // tasks, which need an entered runtime context here in setup.
            let state =
                tauri::async_runtime::block_on(async { AppState::new(store, secrets, data_dir) });
            forward_sync_events(app.handle().clone(), &state);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::meta::app_version,
            commands::accounts::list_accounts,
            commands::accounts::create_account,
            commands::accounts::delete_account,
            commands::connection::test_connection,
            commands::messages::list_messages,
            commands::messages::mark_message_seen,
            commands::detail::get_message_detail,
            commands::sync::sync_account,
            commands::compose::send_message,
            commands::compose::retry_outbox,
            commands::compose::list_outbox,
            commands::drafts::save_draft,
            commands::drafts::list_drafts,
            commands::drafts::delete_draft,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = outcome {
        eprintln!("fatal: failed to start axomail: {error}");
        std::process::exit(1);
    }
}

/// Spawns a task that mirrors the sync engine's broadcast events onto the
/// [`SYNC_EVENT`] Tauri channel. It ends when the engine (and its broadcast
/// sender) is dropped at shutdown.
fn forward_sync_events(app: tauri::AppHandle, state: &AppState) {
    let mut events = state.engine().subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => match SyncEventDto::from_event(&event) {
                    Ok(dto) => {
                        if let Err(error) = app.emit(SYNC_EVENT, dto) {
                            tracing::warn!(error = %error, "failed to emit sync event");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "dropping sync event with out-of-range id");
                    }
                },
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "sync event forwarder fell behind");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}
