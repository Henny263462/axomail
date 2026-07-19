//! Identity commands: list, create, update, delete, and default selection.
//!
//! Each mutation validates the identity's address (and Reply-To, when set) as
//! an email before touching storage; the exactly-one-default invariant is
//! upheld by the repository, so these commands only ask for a default change,
//! never juggle flags themselves.

use axomail_core::{AccountId, EmailAddress, MailError, MailboxId};
use axomail_storage::{IdentityFields, IdentityId, NewIdentity, Store};

use crate::commands::map_command_error;
use crate::dto::{IdentityDto, IdentityInput};
use crate::state::AppState;

/// Lists every identity across all accounts (default first within each), for
/// both the settings editor and the composer's From picker.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_identities_impl(store: &Store) -> Result<Vec<IdentityDto>, MailError> {
    store
        .identities()
        .list_all()
        .await?
        .into_iter()
        .map(IdentityDto::from_domain)
        .collect()
}

/// Creates an identity for an existing account and returns it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an invalid address or an unknown
/// account, and [`MailError::Database`] on any query failure.
pub async fn create_identity_impl(
    store: &Store,
    input: IdentityInput,
) -> Result<IdentityDto, MailError> {
    let account = AccountId::new(input.account_id);
    if store.accounts().get(account).await?.is_none() {
        return Err(MailError::Configuration {
            message: "cannot add an identity to an account that does not exist".to_owned(),
        });
    }
    let fields = build_fields(&input)?;
    // Append after the account's existing identities.
    let sort_position = i64::try_from(store.identities().list(account).await?.len()).unwrap_or(0);
    let identity = store
        .identities()
        .insert(&NewIdentity {
            account,
            fields,
            make_default: input.make_default,
            sort_position,
        })
        .await?;
    IdentityDto::from_domain(identity)
}

/// Updates an identity's content (and, when requested, makes it the default),
/// returning the refreshed identity.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an invalid address or an unknown
/// identity, and [`MailError::Database`] on any query failure.
pub async fn update_identity_impl(
    store: &Store,
    identity_id: i64,
    input: IdentityInput,
) -> Result<IdentityDto, MailError> {
    let id = IdentityId::new(identity_id);
    let fields = build_fields(&input)?;
    if !store.identities().update(id, &fields).await? {
        return Err(MailError::Configuration {
            message: "cannot update an identity that does not exist".to_owned(),
        });
    }
    if input.make_default {
        store.identities().set_default(id).await?;
    }
    let identity = store
        .identities()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Database {
            message: "update_identity: identity missing after update".to_owned(),
        })?;
    IdentityDto::from_domain(identity)
}

/// Deletes an identity. Removing an absent identity is a silent no-op; the
/// repository promotes a survivor to default when the removed one was it.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_identity_impl(store: &Store, identity_id: i64) -> Result<(), MailError> {
    store
        .identities()
        .delete(IdentityId::new(identity_id))
        .await?;
    Ok(())
}

/// Marks an identity as its account's default.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown identity and
/// [`MailError::Database`] on any query failure.
pub async fn set_default_identity_impl(store: &Store, identity_id: i64) -> Result<(), MailError> {
    if !store
        .identities()
        .set_default(IdentityId::new(identity_id))
        .await?
    {
        return Err(MailError::Configuration {
            message: "cannot set a default for an identity that does not exist".to_owned(),
        });
    }
    Ok(())
}

/// Validates and normalizes editor input into storable identity fields.
fn build_fields(input: &IdentityInput) -> Result<IdentityFields, MailError> {
    let email = EmailAddress::parse(input.email.trim())?;
    let reply_to = match input.reply_to.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => {
            // Validate but keep the string form storage uses for addresses.
            EmailAddress::parse(value)?;
            Some(value.to_owned())
        }
        _ => None,
    };
    let signature_text = input
        .signature_text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    Ok(IdentityFields {
        email: email.as_str().to_owned(),
        display_name: input.display_name.trim().to_owned(),
        reply_to,
        signature_text,
        signature_placement: input.signature_placement,
        reply_position: input.reply_position,
        forward_mode: input.forward_mode,
        archive_granularity: input.archive_granularity,
        sent_dest: input.sent_dest.map(MailboxId::new),
        drafts_dest: input.drafts_dest.map(MailboxId::new),
        templates_dest: input.templates_dest.map(MailboxId::new),
        archive_dest: input.archive_dest.map(MailboxId::new),
    })
}

/// Lists every identity across all accounts.
#[tauri::command]
pub async fn list_identities(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<IdentityDto>, String> {
    list_identities_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Creates a new identity.
#[tauri::command]
pub async fn create_identity(
    state: tauri::State<'_, AppState>,
    input: IdentityInput,
) -> Result<IdentityDto, String> {
    create_identity_impl(state.store(), input)
        .await
        .map_err(map_command_error)
}

/// Updates an existing identity.
#[tauri::command]
pub async fn update_identity(
    state: tauri::State<'_, AppState>,
    identity_id: i64,
    input: IdentityInput,
) -> Result<IdentityDto, String> {
    update_identity_impl(state.store(), identity_id, input)
        .await
        .map_err(map_command_error)
}

/// Deletes an identity.
#[tauri::command]
pub async fn delete_identity(
    state: tauri::State<'_, AppState>,
    identity_id: i64,
) -> Result<(), String> {
    delete_identity_impl(state.store(), identity_id)
        .await
        .map_err(map_command_error)
}

/// Marks an identity as its account's default.
#[tauri::command]
pub async fn set_default_identity(
    state: tauri::State<'_, AppState>,
    identity_id: i64,
) -> Result<(), String> {
    set_default_identity_impl(state.store(), identity_id)
        .await
        .map_err(map_command_error)
}
