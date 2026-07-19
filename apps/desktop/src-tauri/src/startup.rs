//! One-time and recurring startup services: launch the background scheduler,
//! resume the outbox, and poll it for scheduled ("send later") messages that
//! have come due.
//!
//! The outbox resume re-sends every row a previous run left in the `queued`
//! (pending) state exactly once, through the same guarded send flow the
//! composer uses; rows in `failed` are never auto-retried. Both the resume and
//! the recurring poller select only rows that are **due** (`list_queued`
//! applies the `next_attempt_unix` time guard), so a message scheduled for
//! later stays put until its instant arrives.

use std::time::Duration;

use axomail_storage::Store;

use crate::commands::compose::retry_outbox_impl;
use crate::settings;
use crate::state::{AppState, SharedSecretStore};

/// How often the poller checks for scheduled messages that have come due.
const SEND_LATER_POLL: Duration = Duration::from_secs(30);

/// Submits every `queued` outbox row that is currently due, once each, through
/// the guarded send flow. Returns the number of rows a send was attempted for.
///
/// A scheduled row whose time has not arrived is skipped by `list_queued`'s
/// time guard. Each row is submitted independently: a failure (for example, an
/// account removed since the row was queued) is logged and skipped so one bad
/// row cannot block the rest. The guarded transition inside the flow is the
/// duplicate-send protection.
///
/// `extra_roots` is empty in production; tests inject a local SMTP server's
/// self-signed certificate through it, mirroring the send commands.
pub async fn drain_due_outbox(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
) -> u64 {
    let due = match store.outbox().list_queued().await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "could not list due outbox rows");
            return 0;
        }
    };
    let mut attempted = 0u64;
    for record in due {
        match retry_outbox_impl(store, secrets, extra_roots, record.id.value()).await {
            Ok(_) => attempted += 1,
            Err(error) => {
                tracing::warn!(error = %error, "could not send a due outbox row");
            }
        }
    }
    attempted
}

/// Recovers rows interrupted mid-submission, then submits every due `queued`
/// row once. Returns the number of rows a send was attempted for.
pub async fn resume_pending_outbox(
    store: &Store,
    secrets: &SharedSecretStore,
    extra_roots: &[Vec<u8>],
) -> u64 {
    // A crash mid-submission leaves rows in `sending`; recover them to a
    // retryable `failed` state (ambiguity documented on the repo method)
    // before resuming the unambiguous, due `queued` rows.
    match store.outbox().recover_interrupted().await {
        Ok(0) => {}
        Ok(count) => {
            tracing::warn!(count, "recovered outbox rows interrupted mid-submission");
        }
        Err(error) => {
            tracing::warn!(error = %error, "could not recover interrupted outbox rows");
        }
    }
    drain_due_outbox(store, secrets, extra_roots).await
}

/// Launches the background scheduler at the persisted interval, resumes the
/// outbox, and starts the scheduled-send poller. Failures are logged, never
/// fatal: a client that cannot start these background helpers is still usable.
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
    spawn_send_later_poller(state.clone());
}

/// Spawns the recurring poller that submits scheduled messages as they come
/// due. The task lives for the process lifetime; the duplicate-send guard makes
/// it safe alongside the one-shot startup resume.
fn spawn_send_later_poller(state: AppState) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(SEND_LATER_POLL);
        // The first tick fires immediately; the startup resume already covered
        // "now", so consume it and act only on subsequent ticks.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let sent = drain_due_outbox(state.store(), state.secrets(), &[]).await;
            if sent > 0 {
                tracing::info!(count = sent, "submitted scheduled messages that came due");
            }
        }
    });
}
