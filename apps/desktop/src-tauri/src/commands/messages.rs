//! Message listing and unread-count queries.

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
    // One batched tags-for-page query keeps row chips O(page), not O(rows).
    let ids: Vec<MessageId> = page.items.iter().map(|envelope| envelope.id).collect();
    let tags = store.tags().tags_for_messages(&ids).await?;
    MessageListPageDto::from_page_with_tags(&page, &tags)
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
