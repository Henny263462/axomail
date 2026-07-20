//! Contact-group (mailing-list) commands.

use std::collections::HashSet;

use axomail_core::MailError;
use axomail_storage::{ContactGroupId, ContactId, Store};

use crate::commands::map_command_error;
use crate::dto::ContactGroupDto;
use crate::state::AppState;

/// Lists every group with its member count.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_contact_groups_impl(store: &Store) -> Result<Vec<ContactGroupDto>, MailError> {
    store
        .contacts()
        .list_groups()
        .await?
        .iter()
        .map(ContactGroupDto::from_group)
        .collect()
}

/// Creates a group.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a blank or duplicate name and
/// [`MailError::Database`] on any query failure.
pub async fn create_contact_group_impl(
    store: &Store,
    name: String,
) -> Result<ContactGroupDto, MailError> {
    let group = store.contacts().create_group(&name).await?;
    ContactGroupDto::from_group(&group)
}

/// Deletes a group (its memberships cascade; member contacts are untouched).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_contact_group_impl(store: &Store, group_id: i64) -> Result<(), MailError> {
    store
        .contacts()
        .delete_group(ContactGroupId::new(group_id))
        .await?;
    Ok(())
}

/// Replaces a contact's group memberships with `group_ids` (adds the missing,
/// removes the extra). Unknown group ids are ignored by the FK layer on add.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown contact and
/// [`MailError::Database`] on any query failure.
pub async fn set_contact_groups_impl(
    store: &Store,
    contact_id: i64,
    group_ids: Vec<i64>,
) -> Result<(), MailError> {
    let contact = ContactId::new(contact_id);
    if store.contacts().get(contact).await?.is_none() {
        return Err(MailError::Configuration {
            message: "cannot set groups for a contact that does not exist".to_owned(),
        });
    }
    let desired: HashSet<i64> = group_ids.into_iter().collect();
    let current: HashSet<i64> = store
        .contacts()
        .groups_of(contact)
        .await?
        .iter()
        .map(|group| group.value())
        .collect();
    for &group in desired.difference(&current) {
        store
            .contacts()
            .add_member(ContactGroupId::new(group), contact)
            .await?;
    }
    for &group in current.difference(&desired) {
        store
            .contacts()
            .remove_member(ContactGroupId::new(group), contact)
            .await?;
    }
    Ok(())
}

/// Lists contact groups.
#[tauri::command]
pub async fn list_contact_groups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContactGroupDto>, String> {
    list_contact_groups_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Creates a contact group.
#[tauri::command]
pub async fn create_contact_group(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<ContactGroupDto, String> {
    create_contact_group_impl(state.store(), name)
        .await
        .map_err(map_command_error)
}

/// Deletes a contact group.
#[tauri::command]
pub async fn delete_contact_group(
    state: tauri::State<'_, AppState>,
    group_id: i64,
) -> Result<(), String> {
    delete_contact_group_impl(state.store(), group_id)
        .await
        .map_err(map_command_error)
}

/// Sets a contact's group memberships.
#[tauri::command]
pub async fn set_contact_groups(
    state: tauri::State<'_, AppState>,
    contact_id: i64,
    group_ids: Vec<i64>,
) -> Result<(), String> {
    set_contact_groups_impl(state.store(), contact_id, group_ids)
        .await
        .map_err(map_command_error)
}
