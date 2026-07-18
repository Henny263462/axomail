//! The draft commands: `save_draft`, `list_drafts`, `delete_draft`.
//!
//! Drafts store whatever the composer currently holds, including incomplete
//! or not-yet-valid recipient lists, so — unlike the send path — they do not
//! validate addresses. The authoritative parsed form is rebuilt only when a
//! draft is actually sent.

use axomail_core::MailError;
use axomail_storage::{DraftId, NewDraft, Store};

use crate::commands::map_command_error;
use crate::dto::{ComposeInput, DraftDto, checked_id};
use crate::state::AppState;

/// Builds the storage row for a compose payload.
fn new_draft(input: ComposeInput) -> NewDraft {
    NewDraft {
        account: axomail_core::AccountId::new(input.account_id),
        subject: input.subject,
        body_text: input.text_body,
        to: input.to,
        cc: input.cc,
        bcc: input.bcc,
    }
}

/// Saves a draft: updates the one identified by `draft_id`, or inserts a new
/// draft when `draft_id` is `None`. Returns the draft's id.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if `draft_id` names a draft that does
/// not exist, [`MailError::Database`] on a query failure, and a safe-integer
/// error if the resulting id cannot cross to the frontend.
pub async fn save_draft_impl(
    store: &Store,
    input: ComposeInput,
    draft_id: Option<i64>,
) -> Result<i64, MailError> {
    let new = new_draft(input);
    match draft_id {
        Some(id) => {
            let draft = DraftId::new(id);
            if !store.drafts().update(draft, &new).await? {
                return Err(MailError::Configuration {
                    message: "cannot update a draft that does not exist".to_owned(),
                });
            }
            checked_id(id)
        }
        None => {
            let record = store.drafts().insert(&new).await?;
            checked_id(record.id.value())
        }
    }
}

/// Lists every draft, most recently updated first.
///
/// # Errors
///
/// Returns [`MailError::Database`] on a query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_drafts_impl(store: &Store) -> Result<Vec<DraftDto>, MailError> {
    store
        .drafts()
        .list()
        .await?
        .into_iter()
        .map(DraftDto::from_record)
        .collect()
}

/// Deletes a draft by id. Deleting an absent draft is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on a query failure.
pub async fn delete_draft_impl(store: &Store, draft_id: i64) -> Result<(), MailError> {
    store.drafts().delete(DraftId::new(draft_id)).await?;
    Ok(())
}

/// Saves (creates or updates) a draft and returns its id.
#[tauri::command]
pub async fn save_draft(
    state: tauri::State<'_, AppState>,
    input: ComposeInput,
    draft_id: Option<i64>,
) -> Result<i64, String> {
    save_draft_impl(state.store(), input, draft_id)
        .await
        .map_err(map_command_error)
}

/// Lists every draft.
#[tauri::command]
pub async fn list_drafts(state: tauri::State<'_, AppState>) -> Result<Vec<DraftDto>, String> {
    list_drafts_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Deletes a draft.
#[tauri::command]
pub async fn delete_draft(state: tauri::State<'_, AppState>, draft_id: i64) -> Result<(), String> {
    delete_draft_impl(state.store(), draft_id)
        .await
        .map_err(map_command_error)
}
