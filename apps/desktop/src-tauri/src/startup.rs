//! One-time startup services: launch the background scheduler and resume the
//! outbox.
//!
//! These run once, from the Tauri setup hook, after the sync engine has taken
//! over crash recovery. The outbox resume re-sends every row a previous run
//! left in the `queued` (pending) state exactly once, through the same guarded
//! send flow the composer uses; rows in `failed` are never auto-retried.

use axomail_storage::Store;

use crate::commands::compose::retry_outbox_impl;
use crate::settings;
use crate::state::{AppState, SharedSecretStore};

/// Re-sends every `queued` outbox row once through the guarded send flow.
/// Returns the number of rows a send was attempted for.
///
/// Each row is submitted independently: a failure (for example, an account
/// removed since the row was queued) is logged and skipped so one bad row
/// cannot block the rest. `failed` rows are never touched — only the user's
/// explicit retry re-runs those.
///
/// `extra_roots` is empty in production; tests inject a local SMTP server's
/// self-signed certificate through it, mirroring the send commands.
pub async fn resume_pending_outbox(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
) -> u64 {
    let queued = match store.outbox().list_queued().await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "could not list queued outbox rows to resume");
            return 0;
        }
    };
    let mut attempted = 0u64;
    for record in queued {
        // The guarded transition inside the flow is the duplicate-send
        // protection; feeding only `queued` rows keeps `failed` rows untouched.
        match retry_outbox_impl(store, secrets, extra_roots, record.id.value()).await {
            Ok(_) => attempted += 1,
            Err(error) => {
                tracing::warn!(error = %error, "could not resume a queued outbox row");
            }
        }
    }
    attempted
}

/// Launches the background scheduler at the persisted interval and resumes the
/// outbox. Failures are logged, never fatal: a client that cannot start these
/// background helpers is still usable.
pub async fn start_background_services(state: &AppState) {
    let interval = settings::sync_interval_minutes(state.store())
        .await
        .unwrap_or(settings::DEFAULT_SYNC_INTERVAL_MINUTES);
    if let Err(error) = state.scheduler().launch(interval).await {
        tracing::warn!(error = %error, "could not launch the background scheduler");
    }
    let resumed = resume_pending_outbox(state.store(), state.secrets(), &[]).await;
    if resumed > 0 {
        tracing::info!(count = resumed, "resumed queued outbox messages on startup");
    }
}
