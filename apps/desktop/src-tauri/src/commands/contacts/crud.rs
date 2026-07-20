//! Contact CRUD commands.

use axomail_core::MailError;
use axomail_storage::{ContactId, ContactUpdate, NewContact, Store};

use super::{contact_dto, parse_emails};
use crate::commands::map_command_error;
use crate::dto::{ContactDetailDto, ContactDto, ContactInput};
use crate::state::AppState;

/// Default cap on the address-book listing (mirrors the repo cap).
const LIST_LIMIT: u32 = 1000;

/// Lists contacts alphabetically, optionally filtered by a name/email
/// substring (the address-book search box).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_contacts_impl(
    store: &Store,
    query: String,
) -> Result<Vec<ContactDto>, MailError> {
    store
        .contacts()
        .list(&query, LIST_LIMIT)
        .await?
        .iter()
        .map(ContactDto::from_contact)
        .collect()
}

/// Loads one contact with the ids of the groups it belongs to (the editor).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown contact and
/// [`MailError::Database`] on any query failure.
pub async fn get_contact_impl(
    store: &Store,
    contact_id: i64,
) -> Result<ContactDetailDto, MailError> {
    let id = ContactId::new(contact_id);
    let contact = contact_dto(store, id).await?;
    let group_ids = store
        .contacts()
        .groups_of(id)
        .await?
        .iter()
        .map(|group| crate::dto::checked_id(group.value()))
        .collect::<Result<Vec<_>, MailError>>()?;
    Ok(ContactDetailDto { contact, group_ids })
}

/// Creates a contact from validated input.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a malformed address and
/// [`MailError::Database`] on any query failure.
pub async fn create_contact_impl(
    store: &Store,
    input: ContactInput,
) -> Result<ContactDto, MailError> {
    let emails = parse_emails(&input)?;
    let contact = store
        .contacts()
        .create(&NewContact {
            display_name: input.display_name,
            emails,
            note: input.note,
        })
        .await?;
    ContactDto::from_contact(&contact)
}

/// Fully replaces a contact's name, note, and addresses.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a malformed address or an unknown
/// contact, and [`MailError::Database`] on any query failure.
pub async fn update_contact_impl(
    store: &Store,
    contact_id: i64,
    input: ContactInput,
) -> Result<ContactDto, MailError> {
    let emails = parse_emails(&input)?;
    let contact = store
        .contacts()
        .update(
            ContactId::new(contact_id),
            &ContactUpdate {
                display_name: input.display_name,
                emails,
                note: input.note,
            },
        )
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot update a contact that does not exist".to_owned(),
        })?;
    ContactDto::from_contact(&contact)
}

/// Deletes a contact, cascading to its addresses and memberships. Deleting an
/// absent contact is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_contact_impl(store: &Store, contact_id: i64) -> Result<(), MailError> {
    store.contacts().delete(ContactId::new(contact_id)).await?;
    Ok(())
}

/// Lists contacts (optionally filtered).
#[tauri::command]
pub async fn list_contacts(
    state: tauri::State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<ContactDto>, String> {
    list_contacts_impl(state.store(), query.unwrap_or_default())
        .await
        .map_err(map_command_error)
}

/// Loads one contact with its group memberships.
#[tauri::command]
pub async fn get_contact(
    state: tauri::State<'_, AppState>,
    contact_id: i64,
) -> Result<ContactDetailDto, String> {
    get_contact_impl(state.store(), contact_id)
        .await
        .map_err(map_command_error)
}

/// Creates a contact.
#[tauri::command]
pub async fn create_contact(
    state: tauri::State<'_, AppState>,
    input: ContactInput,
) -> Result<ContactDto, String> {
    create_contact_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Updates a contact.
#[tauri::command]
pub async fn update_contact(
    state: tauri::State<'_, AppState>,
    contact_id: i64,
    input: ContactInput,
) -> Result<ContactDto, String> {
    update_contact_impl(state.store(), contact_id, input)
        .await
        .map_err(map_command_error)
}

/// Deletes a contact.
#[tauri::command]
pub async fn delete_contact(
    state: tauri::State<'_, AppState>,
    contact_id: i64,
) -> Result<(), String> {
    delete_contact_impl(state.store(), contact_id)
        .await
        .map_err(map_command_error)
}
