//! Message listing and flag commands.

use axomail_core::{MailError, MailboxId, MessageId};
use axomail_storage::{MessageCursor, Store};

use crate::commands::map_command_error;
use crate::dto::MessageListPageDto;
use crate::state::AppState;

/// Lists one page of a mailbox's messages, newest first.
///
/// `cursor` is the opaque token from the previous page, or `None` for the
/// first page; `limit` is clamped to at least one row by the storage layer.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a malformed cursor and
/// [`MailError::Database`] on any query failure.
pub async fn list_messages_impl(
    store: &Store,
    mailbox_id: i64,
    cursor: Option<String>,
    limit: u32,
) -> Result<MessageListPageDto, MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    let cursor = match cursor.as_deref() {
        Some(token) => Some(MessageCursor::parse_token(token)?),
        None => None,
    };
    let page = store.messages().list_page(mailbox, cursor, limit).await?;
    MessageListPageDto::from_page(&page)
}

/// Sets or clears the `seen` flag of a message, preserving its other flags.
///
/// The operation is idempotent: a message that no longer exists, or that
/// already has the requested state, is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn mark_message_seen_impl(
    store: &Store,
    message_id: i64,
    seen: bool,
) -> Result<(), MailError> {
    let id = MessageId::new(message_id);
    let Some(envelope) = store.messages().get(id).await? else {
        return Ok(());
    };
    if envelope.flags.seen == seen {
        return Ok(());
    }
    let mut flags = envelope.flags;
    flags.seen = seen;
    store.messages().update_flags(id, &flags).await?;
    Ok(())
}

/// Lists one page of a mailbox's messages.
#[tauri::command]
pub async fn list_messages(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
    cursor: Option<String>,
    limit: u32,
) -> Result<MessageListPageDto, String> {
    list_messages_impl(state.store(), mailbox_id, cursor, limit)
        .await
        .map_err(map_command_error)
}

/// Sets or clears the `seen` flag of a message.
#[tauri::command]
pub async fn mark_message_seen(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    seen: bool,
) -> Result<(), String> {
    mark_message_seen_impl(state.store(), message_id, seen)
        .await
        .map_err(map_command_error)
}
