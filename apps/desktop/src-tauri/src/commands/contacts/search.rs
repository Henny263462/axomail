//! Recipient autocomplete: contact-address and group-name suggestions for the
//! composer's To/Cc/Bcc fields.

use axomail_core::{EmailAddress, MailError};
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::dto::RecipientSuggestionDto;
use crate::state::AppState;

/// Default number of suggestions returned.
const DEFAULT_LIMIT: u32 = 8;
/// Upper bound so a caller cannot request an unbounded scan.
const MAX_LIMIT: u32 = 25;

/// Builds recipient suggestions for a typed prefix: matching groups (whose
/// name starts with the query) first, then contact addresses. A blank query
/// yields nothing.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure or a bad decode.
pub async fn search_recipients_impl(
    store: &Store,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<RecipientSuggestionDto>, MailError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut suggestions = Vec::new();

    // Group-name prefix matches, expanded to their members' addresses. A group
    // with no deliverable member is dropped (nothing to insert).
    let needle = trimmed.to_lowercase();
    for group in store.contacts().list_groups().await? {
        if suggestions.len() as u32 >= limit {
            break;
        }
        if !group.name.to_lowercase().starts_with(&needle) {
            continue;
        }
        let addresses = store.contacts().group_addresses(group.id).await?;
        if addresses.is_empty() {
            continue;
        }
        let value = addresses
            .iter()
            .map(EmailAddress::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let count = addresses.len();
        suggestions.push(RecipientSuggestionDto {
            kind: "group",
            label: group.name,
            detail: format!("{count} recipient{}", if count == 1 { "" } else { "s" }),
            value,
        });
    }

    // Contact-address matches fill the remainder.
    let remaining = limit - suggestions.len() as u32;
    if remaining > 0 {
        for m in store
            .contacts()
            .search_recipients(trimmed, remaining)
            .await?
        {
            suggestions.push(RecipientSuggestionDto {
                kind: "contact",
                label: m.display_name,
                detail: m.email.as_str().to_owned(),
                value: m.email.as_str().to_owned(),
            });
        }
    }
    Ok(suggestions)
}

/// Returns recipient autocomplete suggestions for a typed prefix.
#[tauri::command]
pub async fn search_recipients(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<RecipientSuggestionDto>, String> {
    search_recipients_impl(state.store(), query, limit)
        .await
        .map_err(map_command_error)
}
