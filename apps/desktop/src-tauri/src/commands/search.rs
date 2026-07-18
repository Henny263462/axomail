//! The `search_messages` command: full-text search joined with envelope data.
//!
//! [`axomail_storage::SearchRepo`] returns lightweight hits (message id,
//! mailbox id, snippet); this layer joins each hit with its stored envelope
//! and owning account through the existing repositories so the results view
//! can show subject, sender, and date without a second round-trip.

use std::collections::HashMap;

use axomail_core::{AccountId, MailError, MailboxId};
use axomail_storage::Store;

use crate::commands::map_command_error;
use crate::dto::SearchResultDto;
use crate::state::AppState;

/// Searches across every account and joins each hit with its envelope.
///
/// A hit whose message or mailbox has vanished since the index matched is
/// skipped rather than failing the whole search.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Configuration`] if a stored id is out of the safe range.
pub async fn search_messages_impl(
    store: &Store,
    query: String,
    limit: u32,
) -> Result<Vec<SearchResultDto>, MailError> {
    let hits = store.search().search(&[], &query, limit).await?;
    let mut accounts: HashMap<MailboxId, AccountId> = HashMap::new();
    let mut results = Vec::with_capacity(hits.len());
    for hit in &hits {
        let Some(envelope) = store.messages().get(hit.message_id).await? else {
            continue;
        };
        let account_id = match accounts.get(&hit.mailbox_id) {
            Some(account) => *account,
            None => {
                let Some(mailbox) = store.mailboxes().get(hit.mailbox_id).await? else {
                    continue;
                };
                accounts.insert(hit.mailbox_id, mailbox.account);
                mailbox.account
            }
        };
        results.push(SearchResultDto::from_parts(
            hit,
            &envelope,
            account_id.value(),
        )?);
    }
    Ok(results)
}

/// Full-text searches stored messages, newest-relevant first.
#[tauri::command]
pub async fn search_messages(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: u32,
) -> Result<Vec<SearchResultDto>, String> {
    search_messages_impl(state.store(), query, limit)
        .await
        .map_err(map_command_error)
}
