//! Contact / address-book commands: CRUD, recipient autocomplete, groups,
//! vCard import/export, and the sent-recipient auto-collection hook.
//!
//! Logic is split across submodules to stay within the file-size gate; each
//! carries its own `*_impl` (unit-tested) and thin `#[tauri::command]` wrapper.
//! The wrappers are re-exported here so `commands::contacts::<name>` resolves in
//! `generate_handler!` and the integration tests.

mod collect;
mod crud;
mod groups;
mod search;
mod vcard;

// Glob re-exports (not named) so the hidden helpers the `#[tauri::command]`
// macro generates next to each command (`__cmd__<name>`,
// `__tauri_command_name_<name>`) are reachable at `commands::contacts::<name>`,
// where `generate_handler!` in `lib.rs` resolves them. This also re-exports the
// `*_impl` functions the integration tests import from `commands::contacts`.
pub use collect::*;
pub use crud::*;
pub use groups::*;
pub use search::*;
pub use vcard::*;

use axomail_core::{EmailAddress, MailError};
use axomail_storage::{NewContactEmail, Store};

use crate::dto::{ContactDto, ContactInput};

/// Maps a validated [`ContactInput`] into the storage layer's address list,
/// rejecting any malformed address up front.
fn parse_emails(input: &ContactInput) -> Result<Vec<NewContactEmail>, MailError> {
    input
        .emails
        .iter()
        .map(|entry| {
            Ok(NewContactEmail {
                email: EmailAddress::parse(entry.email.trim())?,
                label: entry.label.trim().to_owned(),
                is_primary: entry.is_primary,
            })
        })
        .collect()
}

/// Loads a contact and maps it to its DTO, or returns a not-found error.
async fn contact_dto(
    store: &Store,
    id: axomail_storage::ContactId,
) -> Result<ContactDto, MailError> {
    let contact = store
        .contacts()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the contact does not exist".to_owned(),
        })?;
    ContactDto::from_contact(&contact)
}
