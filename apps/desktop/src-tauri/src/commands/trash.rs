//! Trash lifecycle: Empty Trash (the one place server-side removal happens),
//! Compact (reclaim orphaned raw `.eml` blobs), and the "empty Trash on exit"
//! per-account setting (track 3.2).
//!
//! # Why Empty Trash is the only server-side removal
//!
//! Every other action is reversible on the server (move/copy/delete-to-Trash
//! are relocations). Empty Trash is the deliberate, user-initiated disposal:
//! it issues IMAP `STORE \Deleted` + `EXPUNGE` on the **Trash mailbox only**
//! and then drops the local rows and raw bodies. Because IMAP `EXPUNGE` acts
//! solely on the selected mailbox, the adapter selecting Trash before expunging
//! confines the purge to it — a server without UIDPLUS can never expunge
//! `\Deleted` messages in any other folder (see
//! `axomail_imap::message_ops::empty_selected` and ADR-0013). The UI gates this
//! behind a typed confirmation.

use std::collections::HashSet;
use std::path::Path;

use axomail_core::{
    Account, IncomingMailProvider, IncomingProtocol, MailError, MailboxId, MailboxRole, MessageId,
};
use axomail_storage::{Store, raw_message_path, remove_raw};
use axomail_sync::ProviderFactory;

use crate::commands::map_command_error;
use crate::dto::CompactOutcomeDto;
use crate::settings;
use crate::state::AppState;

/// Shorthand for a user-facing configuration error.
fn config(message: &str) -> MailError {
    MailError::Configuration {
        message: message.to_owned(),
    }
}

/// Permanently empties a Trash folder: server expunge (IMAP), then local rows
/// and raw bodies. Returns how many local rows were removed.
///
/// Guarded to `role == Trash` so no other folder can be server-side purged.
/// POP3/local accounts skip the server step (they have no server Trash).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder or a non-Trash
/// folder, propagates the server `EXPUNGE` error, and returns
/// [`MailError::Database`]/[`MailError::Filesystem`] on local failure.
pub async fn empty_trash_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    folder_id: i64,
) -> Result<u64, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let folder = store
        .mailboxes()
        .get(MailboxId::new(folder_id))
        .await?
        .ok_or_else(|| config("the folder does not exist"))?;
    if folder.role != MailboxRole::Trash {
        return Err(config("only the Trash folder can be emptied"));
    }
    let account = store
        .accounts()
        .get(folder.account)
        .await?
        .ok_or_else(|| config("the folder's account no longer exists"))?;

    // Server-first: expunge on the server before touching local state so a
    // failure leaves the Trash intact rather than desynchronized.
    if account.incoming.protocol == IncomingProtocol::Imap {
        expunge_on_server(factory, &account, &folder.remote_name).await?;
    }

    // Delete only *purgeable* rows — those with no un-replayed relocate op still
    // queued. A message moved/deleted to Trash optimistically but not yet
    // replayed still physically sits in its server source folder; deleting its
    // row would cascade away the pending op and resurrect the mail (see
    // `delete_in_mailbox_purgeable` and ADR-0013). Such rows survive to a later
    // Empty Trash after their op replays. Reclaim the raw `.eml` bodies of
    // exactly the rows deleted — never a stale snapshot — so a message that left
    // Trash concurrently keeps its still-live blob.
    let purged = store
        .messages()
        .delete_in_mailbox_purgeable(folder.id)
        .await?;
    let removed = purged.len() as u64;
    for id in purged {
        // Best-effort: a missing or unreadable blob must not fail the purge.
        let _ = remove_raw(&raw_message_path(raw_root, id.value())).await;
    }
    Ok(removed)
}

/// Connects, expunges `mailbox` on the server, and disconnects (best effort),
/// propagating the expunge error after the disconnect.
async fn expunge_on_server<P, F>(
    factory: &F,
    account: &Account,
    mailbox: &axomail_core::MailboxName,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut provider = factory.make(account).await?;
    provider.connect().await?;
    let outcome = provider.expunge_mailbox(mailbox).await;
    let _ = provider.disconnect().await;
    outcome
}

/// Empties the Trash of every account whose "empty Trash on exit" setting is on
/// (best-effort per account; one failure never blocks the others). Run at app
/// close.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the account list cannot be read.
pub async fn empty_trash_on_exit_all<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    for account in store.accounts().list().await? {
        if !settings::empty_trash_on_exit_enabled(store, account.id.value()).await? {
            continue;
        }
        let Some(trash) = store
            .mailboxes()
            .list(account.id)
            .await?
            .into_iter()
            .find(|mailbox| mailbox.role == MailboxRole::Trash)
        else {
            continue;
        };
        if let Err(error) = empty_trash_impl(store, factory, raw_root, trash.id.value()).await {
            tracing::warn!(error = %error, account = account.id.value(), "empty-trash-on-exit failed");
        }
    }
    Ok(())
}

/// Reclaims orphaned raw `.eml` blobs — files under `<raw_root>/raw/` whose
/// message id no longer has a row. Returns how many were removed.
///
/// This is the disk half of the store cleanup: message-row deletes cascade in
/// the database, but the out-of-database bodies are keyed by id and can be left
/// behind (the Wave-1 orphan leak). Temp files from an interrupted atomic write
/// carry a `.tmp` suffix and are skipped.
///
/// # Errors
///
/// Returns [`MailError::Database`] on a query failure and
/// [`MailError::Filesystem`] on a directory-read or removal failure.
pub async fn compact_storage_impl(
    store: &Store,
    raw_root: &Path,
) -> Result<CompactOutcomeDto, MailError> {
    let existing: HashSet<i64> = store
        .messages()
        .all_ids()
        .await?
        .into_iter()
        .map(|id| id.value())
        .collect();
    let reclaimed = reclaim_orphaned_blobs(store, raw_root, &existing).await?;
    Ok(CompactOutcomeDto { reclaimed })
}

/// Scans `<raw_root>/raw/` and unlinks each `.eml` whose message id is orphaned,
/// returning how many were removed. `snapshot` is the up-front set of live ids.
///
/// A candidate whose id is **absent from `snapshot`** is re-checked against the
/// live database immediately before unlinking: the snapshot is taken once at the
/// start, so a message inserted and body-downloaded *after* it (its id therefore
/// missing) would otherwise have its valid blob deleted (the compact TOCTOU).
/// Only a candidate absent from the snapshot **and** confirmed gone by the live
/// re-check is removed. Temp files from an interrupted atomic write carry a
/// `.tmp` suffix and are skipped. Split out (and taking the snapshot as an
/// argument) so a test can drive the stale-snapshot path deterministically.
///
/// # Errors
///
/// Returns [`MailError::Database`] on a query failure and
/// [`MailError::Filesystem`] on a directory-read or removal failure.
pub async fn reclaim_orphaned_blobs(
    store: &Store,
    raw_root: &Path,
    snapshot: &HashSet<i64>,
) -> Result<u64, MailError> {
    let raw_dir = raw_root.join("raw");
    let mut dir = match tokio::fs::read_dir(&raw_dir).await {
        Ok(dir) => dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(0);
        }
        Err(error) => {
            return Err(MailError::Filesystem {
                message: format!("reading raw cache directory: {}", error.kind()),
            });
        }
    };

    let mut reclaimed = 0u64;
    loop {
        let entry = dir
            .next_entry()
            .await
            .map_err(|error| MailError::Filesystem {
                message: format!("scanning raw cache directory: {}", error.kind()),
            })?;
        let Some(entry) = entry else { break };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("eml") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<i64>().ok())
        else {
            continue;
        };
        // A blob absent from the up-front snapshot might belong to a message
        // inserted (and body-downloaded) *after* it was taken. Re-confirm the id
        // is still orphaned against the live database immediately before
        // unlinking, so a concurrently-arrived message never loses its blob.
        if !snapshot.contains(&id)
            && !store.messages().exists(MessageId::new(id)).await?
            && remove_raw(&path).await?
        {
            reclaimed += 1;
        }
    }
    Ok(reclaimed)
}

/// Empties a Trash folder after a typed confirmation in the UI.
#[tauri::command]
pub async fn empty_trash(state: tauri::State<'_, AppState>, folder_id: i64) -> Result<u64, String> {
    empty_trash_impl(state.store(), state.factory(), state.raw_root(), folder_id)
        .await
        .map_err(map_command_error)
}

/// Reclaims orphaned raw `.eml` blobs.
#[tauri::command]
pub async fn compact_storage(
    state: tauri::State<'_, AppState>,
) -> Result<CompactOutcomeDto, String> {
    compact_storage_impl(state.store(), state.raw_root())
        .await
        .map_err(map_command_error)
}

/// Reads an account's "empty Trash on exit" setting.
#[tauri::command]
pub async fn get_empty_trash_on_exit(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<bool, String> {
    settings::empty_trash_on_exit_enabled(state.store(), account_id)
        .await
        .map_err(map_command_error)
}

/// Sets an account's "empty Trash on exit" setting.
#[tauri::command]
pub async fn set_empty_trash_on_exit(
    state: tauri::State<'_, AppState>,
    account_id: i64,
    enabled: bool,
) -> Result<(), String> {
    settings::set_empty_trash_on_exit_enabled(state.store(), account_id, enabled)
        .await
        .map_err(map_command_error)
}
