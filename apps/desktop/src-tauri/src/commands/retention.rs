//! Per-mailbox retention commands: read/write the policy, and a maintenance
//! pass that applies it (track 5.5).
//!
//! The maintenance pass ([`apply_folder_retention`]) never hard-deletes server
//! mail. It reads the mailbox's [`RetentionPolicy`], asks the storage layer for
//! the over-retention message ids, and disposes of them through one of two safe
//! paths:
//!
//! * `Trash` — reuses [`delete_to_trash_impl`], the same guarded write-back
//!   path a manual delete uses (an optimistic local move plus a `pending_ops`
//!   op that replays a *move to Trash* on the server). Messages already in
//!   Trash are skipped (source == target), so running it on the Trash folder
//!   is a no-op.
//! * `PurgeBody` — removes only the locally-cached raw `.eml` body and clears
//!   `has_raw`, exactly like the offline-cache cleanup. Nothing leaves the
//!   server; the body is re-fetched on demand.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::{IncomingMailProvider, MailError, MailboxId};
use axomail_storage::{RetentionAction, RetentionMode, Store, raw_message_path, remove_raw};
use axomail_sync::{ProviderFactory, SyncEngine};

use crate::commands::actions::delete_to_trash_impl;
use crate::commands::map_command_error;
use crate::dto::{RetentionOutcomeDto, RetentionPolicyDto, RetentionPolicyInput};
use crate::state::AppState;

/// Reads a mailbox's retention policy, erroring if the mailbox does not exist.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox and
/// [`MailError::Database`] on a query failure.
pub async fn get_folder_retention_impl(
    store: &Store,
    mailbox_id: i64,
) -> Result<RetentionPolicyDto, MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    ensure_mailbox_exists(store, mailbox).await?;
    let policy = store.retention().get(mailbox).await?;
    RetentionPolicyDto::new(mailbox_id, policy)
}

/// Sets a mailbox's retention policy.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox and
/// [`MailError::Database`] on a query failure.
pub async fn set_folder_retention_impl(
    store: &Store,
    mailbox_id: i64,
    input: RetentionPolicyInput,
) -> Result<(), MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    if !store.retention().set(mailbox, input.into_policy()).await? {
        return Err(MailError::Configuration {
            message: "cannot set a retention policy for a mailbox that does not exist".to_owned(),
        });
    }
    Ok(())
}

/// Applies a mailbox's retention policy once, disposing of the over-retention
/// messages through the policy's safe action. A `keep_all` policy is a no-op.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown mailbox (or, for the
/// `Trash` action, an account with no Trash folder), [`MailError::Database`] on
/// a query failure, [`MailError::Filesystem`] if a cached body cannot be
/// removed, and [`MailError::Cancelled`] if the engine has shut down.
pub async fn apply_folder_retention_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    raw_root: &Path,
    mailbox_id: i64,
) -> Result<RetentionOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox = MailboxId::new(mailbox_id);
    ensure_mailbox_exists(store, mailbox).await?;
    let policy = store.retention().get(mailbox).await?;
    if policy.mode == RetentionMode::KeepAll {
        return Ok(RetentionOutcomeDto {
            moved_to_trash: 0,
            purged_bodies: 0,
        });
    }

    match policy.action {
        RetentionAction::Trash => {
            let ids: Vec<i64> = store
                .retention()
                .over_retention(mailbox, now_unix(), false)
                .await?
                .into_iter()
                .map(|id| id.value())
                .collect();
            if ids.is_empty() {
                return Ok(RetentionOutcomeDto {
                    moved_to_trash: 0,
                    purged_bodies: 0,
                });
            }
            let outcome = delete_to_trash_impl(store, engine, ids).await?;
            Ok(RetentionOutcomeDto {
                moved_to_trash: outcome.moves.len() as u64,
                purged_bodies: 0,
            })
        }
        RetentionAction::PurgeBody => {
            let ids = store
                .retention()
                .over_retention(mailbox, now_unix(), true)
                .await?;
            let mut purged = 0u64;
            for id in ids {
                remove_raw(&raw_message_path(raw_root, id.value())).await?;
                store.offline().clear_has_raw(id).await?;
                purged += 1;
            }
            Ok(RetentionOutcomeDto {
                moved_to_trash: 0,
                purged_bodies: purged,
            })
        }
    }
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

/// Reads a mailbox's retention policy.
#[tauri::command]
pub async fn get_folder_retention(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<RetentionPolicyDto, String> {
    get_folder_retention_impl(state.store(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Sets a mailbox's retention policy.
#[tauri::command]
pub async fn set_folder_retention(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
    policy: RetentionPolicyInput,
) -> Result<(), String> {
    set_folder_retention_impl(state.store(), mailbox_id, policy)
        .await
        .map_err(map_command_error)
}

/// Applies a mailbox's retention policy once and reports what it did.
#[tauri::command]
pub async fn apply_folder_retention(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<RetentionOutcomeDto, String> {
    apply_folder_retention_impl(state.store(), state.engine(), state.raw_root(), mailbox_id)
        .await
        .map_err(map_command_error)
}
