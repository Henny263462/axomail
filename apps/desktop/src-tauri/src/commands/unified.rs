//! The `list_unified_inbox` command: one date-ordered page across every
//! account's inbox-role mailbox.

use axomail_core::MailError;
use axomail_storage::{MessageCursor, Store};

use crate::commands::map_command_error;
use crate::dto::UnifiedPageDto;
use crate::state::AppState;

/// Lists one page of the unified inbox, newest first across all accounts.
///
/// `cursor` is the opaque token from the previous page, or `None` for the
/// first page; `limit` is clamped to at least one row by the storage layer.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a malformed cursor and
/// [`MailError::Database`] on any query failure.
pub async fn list_unified_inbox_impl(
    store: &Store,
    cursor: Option<String>,
    limit: u32,
) -> Result<UnifiedPageDto, MailError> {
    let cursor = match cursor.as_deref() {
        Some(token) => Some(MessageCursor::parse_token(token)?),
        None => None,
    };
    let page = store.unified().list_unified_inbox(cursor, limit).await?;
    UnifiedPageDto::from_page(&page)
}

/// Lists one page of the unified inbox.
#[tauri::command]
pub async fn list_unified_inbox(
    state: tauri::State<'_, AppState>,
    cursor: Option<String>,
    limit: u32,
) -> Result<UnifiedPageDto, String> {
    list_unified_inbox_impl(state.store(), cursor, limit)
        .await
        .map_err(map_command_error)
}
