//! axomail desktop application shell.
//!
//! Hosts the Tauri runtime and exposes the command surface to the Vue
//! frontend. All mail logic lives in the `crates/` workspace members; this
//! crate stays a thin composition layer that wires storage, the keychain,
//! the protocol adapters, and the sync engine together (ADR-0004).

pub mod commands;
pub mod composition;
pub mod dto;
pub mod notifications;
pub mod pop3_sync;
pub mod scheduler;
pub mod settings;
pub mod startup;
pub mod state;

use std::sync::Arc;

use axomail_security::KeyringSecretStore;
use axomail_storage::Store;
use axomail_sync::SyncEvent;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

use crate::dto::SyncEventDto;
use crate::notifications::{NewMailNotifier, TauriNotifier, maybe_notify_new_messages};
use crate::state::{AppState, SharedSecretStore};

/// Name of the SQLite database file within the application data directory.
const DATABASE_FILE: &str = "axomail.db";

/// Tauri event channel carrying [`SyncEventDto`] progress to the frontend.
const SYNC_EVENT: &str = "axomail://sync";

/// Builds and runs the Tauri application.
///
/// The setup hook resolves the application data directory, opens the local
/// store (running migrations), starts the sync engine (whose dispatcher
/// performs crash recovery once), launches the background scheduler, resumes
/// the outbox, and forwards the engine's progress events to the frontend as
/// [`SYNC_EVENT`]. If storage cannot be opened the process exits non-zero
/// without presenting a window: the client is useless without its local
/// database.
pub fn run() {
    let outcome = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        // Opens the OAuth2 consent URL in the system browser (ADR-0009). The
        // capability scopes this to `https://*` only — no path opening or
        // reveal-in-dir.
        .plugin(tauri_plugin_opener::init())
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
            let notifier = TauriNotifier::new(app.handle().clone());
            forward_sync_events(app.handle().clone(), &state, notifier);
            let startup_state = state.clone();
            tauri::async_runtime::spawn(async move {
                startup::start_background_services(&startup_state).await;
            });
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
            commands::remote_content::set_remote_content_allowed,
            commands::sync::sync_account,
            commands::jobs::list_sync_jobs,
            commands::jobs::cancel_sync_job,
            commands::jobs::cancel_all_sync_jobs,
            commands::compose::send_message,
            commands::compose::retry_outbox,
            commands::compose::list_outbox,
            commands::drafts::save_draft,
            commands::drafts::list_drafts,
            commands::drafts::delete_draft,
            commands::search::search_messages,
            commands::unified::list_unified_inbox,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_pop3_delete_policy,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = outcome {
        eprintln!("fatal: failed to start axomail: {error}");
        std::process::exit(1);
    }
}

/// Spawns a task that mirrors the sync engine's broadcast events onto the
/// [`SYNC_EVENT`] Tauri channel and raises an aggregate new-mail notification
/// when a job finishes having ingested new messages. It ends when the engine
/// (and its broadcast sender) is dropped at shutdown.
fn forward_sync_events(
    app: tauri::AppHandle,
    state: &AppState,
    notifier: impl NewMailNotifier + 'static,
) {
    let mut events = state.engine().subscribe();
    let store = state.store().clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    notify_if_new_mail(&store, &notifier, &event).await;
                    forward_one(&app, &event);
                }
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "sync event forwarder fell behind");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

/// Emits one engine event to the frontend as a [`SyncEventDto`].
fn forward_one(app: &tauri::AppHandle, event: &SyncEvent) {
    match SyncEventDto::from_event(event) {
        Ok(dto) => {
            if let Err(error) = app.emit(SYNC_EVENT, dto) {
                tracing::warn!(error = %error, "failed to emit sync event");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "dropping sync event with out-of-range id");
        }
    }
}

/// Shows one aggregate notification when a job finished with new messages and
/// the user has notifications enabled.
async fn notify_if_new_mail(store: &Store, notifier: &impl NewMailNotifier, event: &SyncEvent) {
    let SyncEvent::JobFinished { summary, .. } = event else {
        return;
    };
    if summary.new_messages == 0 {
        return;
    }
    let enabled = match settings::notifications_enabled(store).await {
        Ok(enabled) => enabled,
        Err(error) => {
            tracing::warn!(error = %error, "could not read the notifications setting");
            return;
        }
    };
    maybe_notify_new_messages(notifier, enabled, summary.new_messages);
}
