//! Offline-store commands: the global Work Offline toggle, per-mailbox offline
//! policy read/write, an immediate "download now", and a cache-cleanup purge.
//!
//! Work Offline flips one shared flag through the engine (so an online
//! transition resumes queued jobs) and pauses the scheduler; the persisted
//! setting restores the mode on the next launch. The policy and download-now
//! commands drive the sync-time body download (see
//! [`axomail_storage::OfflinePolicyRepo`]); cleanup is an explicit,
//! user-invoked purge — never automatic data loss.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{IncomingMailProvider, MailError, MailboxId};
use axomail_storage::{OfflineMode, Store, raw_message_path, remove_raw};
use axomail_sync::{ProviderFactory, SyncEngine, SyncMailbox};

use crate::commands::map_command_error;
use crate::dto::OfflinePolicyDto;
use crate::scheduler::SchedulerControl;
use crate::settings;
use crate::state::AppState;

/// Sets the global Work Offline state: persists it, flips the engine gate
/// (which resumes queued jobs when returning online), and pauses/resumes the
/// background scheduler.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the setting cannot be persisted.
pub async fn set_work_offline_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    scheduler: &SchedulerControl,
    offline: bool,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    settings::set_work_offline(store, offline).await?;
    engine.set_offline(offline);
    scheduler.set_offline(offline);
    Ok(())
}

/// Reads a mailbox's offline policy, erroring if the mailbox does not exist.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox or an id out of
/// the safe range, and [`MailError::Database`] on a query failure.
pub async fn get_mailbox_offline_policy_impl(
    store: &Store,
    mailbox_id: i64,
) -> Result<OfflinePolicyDto, MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    ensure_mailbox_exists(store, mailbox).await?;
    let policy = store.offline().get(mailbox).await?;
    OfflinePolicyDto::new(mailbox_id, policy)
}

/// Sets a mailbox's offline policy.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox and
/// [`MailError::Database`] on a query failure.
pub async fn set_mailbox_offline_policy_impl(
    store: &Store,
    mailbox_id: i64,
    mode: OfflineMode,
    recent_days: u32,
) -> Result<(), MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    if !store.offline().set(mailbox, mode, recent_days).await? {
        return Err(MailError::Configuration {
            message: "cannot set an offline policy for a mailbox that does not exist".to_owned(),
        });
    }
    Ok(())
}

/// Marks a mailbox for an immediate full offline download and enqueues a sync
/// job for it, so bodies are cached now regardless of the polling interval or
/// the standing policy. Progress is observable through the normal sync events.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn download_mailbox_now_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    mailbox_id: i64,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox = store
        .mailboxes()
        .get(MailboxId::new(mailbox_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot download a mailbox that does not exist".to_owned(),
        })?;
    store
        .offline()
        .set_pending_full_download(mailbox.id, true)
        .await?;
    engine
        .enqueue(SyncMailbox {
            account_id: mailbox.account,
            mailbox_id: mailbox.id,
        })
        .await?;
    Ok(())
}

/// Purges cached raw bodies that fall outside the mailbox's current offline
/// window (everything for `none`, older-than-window for `recent`), removing the
/// files and clearing `has_raw`. Returns how many messages were purged. This is
/// an explicit, user-invoked reclaim — never run automatically.
///
/// # Errors
///
/// Returns [`MailError::Database`] on a query failure or [`MailError::Filesystem`]
/// if a cached file cannot be removed.
pub async fn cleanup_offline_cache_impl(
    store: &Store,
    raw_root: &Path,
    mailbox_id: i64,
) -> Result<u64, MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    ensure_mailbox_exists(store, mailbox).await?;
    let ids = store
        .offline()
        .cached_outside_window(mailbox, now_unix())
        .await?;
    let mut purged = 0u64;
    for id in ids {
        remove_raw(&raw_message_path(raw_root, id.value())).await?;
        store.offline().clear_has_raw(id).await?;
        purged += 1;
    }
    Ok(purged)
}

/// Errors if `mailbox` does not exist.
async fn ensure_mailbox_exists(store: &Store, mailbox: MailboxId) -> Result<(), MailError> {
    if store.mailboxes().get(mailbox).await?.is_none() {
        return Err(MailError::Configuration {
            message: "the requested mailbox does not exist".to_owned(),
        });
    }
    Ok(())
}

/// Current wall-clock time in Unix seconds (0 before the epoch).
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Sets the global Work Offline mode.
#[tauri::command]
pub async fn set_work_offline(
    state: tauri::State<'_, AppState>,
    offline: bool,
) -> Result<(), String> {
    set_work_offline_impl(state.store(), state.engine(), state.scheduler(), offline)
        .await
        .map_err(map_command_error)
}

/// Reads the current Work Offline mode.
#[tauri::command]
pub async fn get_work_offline(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.is_offline())
}

/// Reads a mailbox's offline policy.
#[tauri::command]
pub async fn get_mailbox_offline_policy(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<OfflinePolicyDto, String> {
    get_mailbox_offline_policy_impl(state.store(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Sets a mailbox's offline policy.
#[tauri::command]
pub async fn set_mailbox_offline_policy(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
    mode: OfflineMode,
    recent_days: u32,
) -> Result<(), String> {
    set_mailbox_offline_policy_impl(state.store(), mailbox_id, mode, recent_days)
        .await
        .map_err(map_command_error)
}

/// Enqueues an immediate full offline download for a mailbox.
#[tauri::command]
pub async fn download_mailbox_now(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<(), String> {
    download_mailbox_now_impl(state.store(), state.engine(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Purges cached bodies outside a mailbox's offline window.
#[tauri::command]
pub async fn cleanup_offline_cache(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<u64, String> {
    cleanup_offline_cache_impl(state.store(), state.raw_root(), mailbox_id)
        .await
        .map_err(map_command_error)
}
