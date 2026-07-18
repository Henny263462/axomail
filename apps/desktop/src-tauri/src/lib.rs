//! axomail desktop application shell.
//!
//! Hosts the Tauri runtime and exposes the command surface to the Vue
//! frontend. All mail logic lives in the `crates/` workspace members; this
//! crate stays a thin composition layer that wires storage, the keychain, and
//! the typed commands together.

pub mod commands;
pub mod dto;
pub mod state;

use std::sync::Arc;

use axomail_security::KeyringSecretStore;
use axomail_storage::Store;
use tauri::Manager;

use crate::state::{AppState, SharedSecretStore};

/// Name of the SQLite database file within the application data directory.
const DATABASE_FILE: &str = "axomail.db";

/// Builds and runs the Tauri application.
///
/// The setup hook resolves the application data directory, opens the local
/// store (running migrations), and manages the shared [`AppState`]. If
/// storage cannot be opened the process exits non-zero without presenting a
/// window: the client is useless without its local database, and continuing
/// would risk showing an empty shell that silently drops every command.
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
            app.manage(AppState::new(store, secrets));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::meta::app_version,
            commands::accounts::list_accounts,
            commands::accounts::create_account,
            commands::accounts::delete_account,
            commands::messages::list_messages,
            commands::messages::mark_message_seen,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = outcome {
        eprintln!("fatal: failed to start axomail: {error}");
        std::process::exit(1);
    }
}
