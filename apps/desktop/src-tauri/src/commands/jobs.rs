//! Activity-manager commands: list sync jobs with display context, cancel a
//! single job, and cancel every tracked job.
//!
//! Listing is a thin read over [`axomail_storage::SyncJobRepo::list_by_state`]
//! that resolves each job's account and mailbox names for display. The two
//! cancel commands are thin wrappers over the engine's cooperative
//! cancellation (`cancel`/`cancel_all`); a cancelled job stops at its next
//! checkpoint and is recorded as `cancelled` (never a failure).

use std::collections::HashMap;

use axomail_core::{IncomingMailProvider, MailError};
use axomail_storage::{JobState, Store, SyncJob, SyncJobId};
use axomail_sync::{ProviderFactory, SyncEngine};

use crate::commands::map_command_error;
use crate::dto::{SyncJobDto, checked_id};
use crate::state::AppState;

/// Upper bound on the recent terminal jobs returned (newest first), so a long
/// history never bloats the activity list.
const RECENT_TERMINAL_LIMIT: usize = 50;

/// Lists sync jobs for the activity manager: every running and pending job,
/// plus the most recent terminal ones, each with its account and mailbox
/// names resolved for display.
///
/// Ordering is running first, then pending (both oldest first), then recent
/// terminal jobs newest first.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe range.
pub async fn list_sync_jobs_impl(store: &Store) -> Result<Vec<SyncJobDto>, MailError> {
    let (account_labels, mailbox_labels) = resolve_labels(store).await?;

    let running = store.sync_jobs().list_by_state(JobState::Running).await?;
    let pending = store.sync_jobs().list_by_state(JobState::Pending).await?;
    let recent = recent_terminal(store).await?;

    let mut jobs = Vec::with_capacity(running.len() + pending.len() + recent.len());
    for job in running.iter().chain(pending.iter()).chain(recent.iter()) {
        jobs.push(SyncJobDto::from_job(job, &account_labels, &mailbox_labels)?);
    }
    Ok(jobs)
}

/// Gathers terminal jobs (succeeded, failed, cancelled), newest first, capped
/// at [`RECENT_TERMINAL_LIMIT`].
async fn recent_terminal(store: &Store) -> Result<Vec<SyncJob>, MailError> {
    let mut terminal = Vec::new();
    for state in [JobState::Succeeded, JobState::Failed, JobState::Cancelled] {
        terminal.extend(store.sync_jobs().list_by_state(state).await?);
    }
    // Newest first: terminal jobs carry a finish time; fall back to id so the
    // order is total and stable even if two rows share a timestamp.
    terminal.sort_by(|a, b| {
        b.finished_at_unix
            .cmp(&a.finished_at_unix)
            .then_with(|| b.id.cmp(&a.id))
    });
    terminal.truncate(RECENT_TERMINAL_LIMIT);
    Ok(terminal)
}

/// Builds id → name maps for every account and mailbox, so a job's context is
/// resolved without one query per job.
async fn resolve_labels(
    store: &Store,
) -> Result<(HashMap<i64, String>, HashMap<i64, String>), MailError> {
    let mut account_labels = HashMap::new();
    let mut mailbox_labels = HashMap::new();
    for account in store.accounts().list().await? {
        for mailbox in store.mailboxes().list(account.id).await? {
            mailbox_labels.insert(mailbox.id.value(), mailbox.name.as_str().to_owned());
        }
        account_labels.insert(account.id.value(), account.display_name);
    }
    Ok((account_labels, mailbox_labels))
}

/// Requests cooperative cancellation of a single job. Cancelling a job that is
/// not tracked (already finished, or unknown) is a harmless no-op.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if `job_id` is out of the safe integer
/// range — such a value cannot name any real job.
pub fn cancel_sync_job_impl<P, F>(engine: &SyncEngine<P, F>, job_id: i64) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let id = checked_id(job_id)?;
    engine.cancel(SyncJobId::new(id));
    Ok(())
}

/// Requests cooperative cancellation of every currently tracked job.
pub fn cancel_all_sync_jobs_impl<P, F>(engine: &SyncEngine<P, F>)
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    engine.cancel_all();
}

/// Lists sync jobs for the activity manager.
#[tauri::command]
pub async fn list_sync_jobs(state: tauri::State<'_, AppState>) -> Result<Vec<SyncJobDto>, String> {
    list_sync_jobs_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Cancels a single sync job by id.
#[tauri::command]
pub async fn cancel_sync_job(state: tauri::State<'_, AppState>, job_id: i64) -> Result<(), String> {
    cancel_sync_job_impl(state.engine(), job_id).map_err(map_command_error)
}

/// Cancels every currently tracked sync job.
#[tauri::command]
pub async fn cancel_all_sync_jobs(state: tauri::State<'_, AppState>) -> Result<(), String> {
    cancel_all_sync_jobs_impl(state.engine());
    Ok(())
}
