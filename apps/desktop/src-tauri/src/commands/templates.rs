//! Message templates: a small reusable-composition library.
//!
//! Templates are persisted as a JSON list under one `app_settings` key. This is
//! the migration-free realization of the "Templates folder role" the plan calls
//! for — the `mailboxes.role` CHECK constraint (migration 0001) admits no new
//! role value and this track ships no migration, so a KV-backed library stands
//! in (see `dto::template`). The composer's "Save as template" calls
//! [`save_template`]; "New message from template" reads [`list_templates`] and
//! pre-fills the composer from the chosen entry.

use axomail_core::MailError;
use axomail_storage::Store;

use super::map_command_error;
use crate::dto::{MessageTemplateDto, NewTemplateInput};
use crate::state::AppState;

/// KV key holding the JSON-encoded template list.
const KEY_TEMPLATES: &str = "message_templates";

/// Loads the stored template list, treating an absent key as empty and a corrupt
/// value as empty (logged) — the list has a single writer, so corruption is not
/// expected.
async fn load(store: &Store) -> Result<Vec<MessageTemplateDto>, MailError> {
    let Some(json) = store.settings().get(KEY_TEMPLATES).await? else {
        return Ok(Vec::new());
    };
    match serde_json::from_str(&json) {
        Ok(templates) => Ok(templates),
        Err(error) => {
            tracing::warn!(%error, "stored templates could not be parsed; treating as empty");
            Ok(Vec::new())
        }
    }
}

/// Persists the template list as JSON.
async fn persist(store: &Store, templates: &[MessageTemplateDto]) -> Result<(), MailError> {
    let json = serde_json::to_string(templates).map_err(|error| MailError::Configuration {
        message: format!("could not encode the template list: {error}"),
    })?;
    store.settings().set(KEY_TEMPLATES, &json).await
}

/// Lists every saved template in insertion order.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn list_templates_impl(store: &Store) -> Result<Vec<MessageTemplateDto>, MailError> {
    load(store).await
}

/// Saves a new template, assigning it a fresh id, and returns it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a blank name and
/// [`MailError::Database`] on any query failure.
pub async fn save_template_impl(
    store: &Store,
    input: NewTemplateInput,
) -> Result<MessageTemplateDto, MailError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(MailError::Configuration {
            message: "a template needs a name".to_owned(),
        });
    }
    let mut templates = load(store).await?;
    let next_id = templates.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let template = MessageTemplateDto {
        id: next_id,
        name: name.to_owned(),
        subject: input.subject,
        to: input.to,
        cc: input.cc,
        bcc: input.bcc,
        body_text: input.body_text,
    };
    templates.push(template.clone());
    persist(store, &templates).await?;
    Ok(template)
}

/// Deletes the template with the given id (a no-op if it does not exist).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_template_impl(store: &Store, template_id: i64) -> Result<(), MailError> {
    let mut templates = load(store).await?;
    let before = templates.len();
    templates.retain(|template| template.id != template_id);
    if templates.len() == before {
        return Ok(());
    }
    persist(store, &templates).await
}

/// Lists every saved message template.
#[tauri::command]
pub async fn list_templates(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<MessageTemplateDto>, String> {
    list_templates_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Saves the composer's current content as a named template.
#[tauri::command]
pub async fn save_template(
    state: tauri::State<'_, AppState>,
    input: NewTemplateInput,
) -> Result<MessageTemplateDto, String> {
    save_template_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Deletes a saved template.
#[tauri::command]
pub async fn delete_template(
    state: tauri::State<'_, AppState>,
    template_id: i64,
) -> Result<(), String> {
    delete_template_impl(state.store(), template_id)
        .await
        .map_err(map_command_error)
}
