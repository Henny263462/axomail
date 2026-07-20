//! vCard import/export commands. The frontend picks a path through the dialog
//! plugin; these commands do the disk IO in Rust `std::fs` (ADR-0010) and map
//! between the address book and the [`axomail_storage`] vCard reader/writer.

use axomail_core::{EmailAddress, MailError};
use axomail_storage::{
    NewContact, NewContactEmail, Store, VCard, VCardEmail, VCardVersion, emit_vcards, parse_vcards,
};

use crate::commands::map_command_error;
use crate::dto::VCardImportDto;
use crate::state::AppState;

/// Cap on how many contacts a single export writes.
const EXPORT_CAP: u32 = 100_000;

/// Reads a vCard file and imports its cards, skipping any card whose address is
/// already in the book. Returns the imported / skipped counts.
///
/// # Errors
///
/// Returns [`MailError::Filesystem`] if the file cannot be read and
/// [`MailError::Database`] on any query failure.
pub async fn import_contacts_vcard_impl(
    store: &Store,
    path: String,
) -> Result<VCardImportDto, MailError> {
    let text = read_file(path).await?;
    let cards = parse_vcards(&text)?;
    let mut imported = 0u32;
    let mut skipped = 0u32;
    for card in &cards {
        let emails = valid_emails(card);
        // Skip a card whose any address is already known (dedup by address).
        let mut already = false;
        for email in &emails {
            if store
                .contacts()
                .find_by_email(&email.email)
                .await?
                .is_some()
            {
                already = true;
                break;
            }
        }
        if already {
            skipped += 1;
            continue;
        }
        store
            .contacts()
            .create(&NewContact {
                display_name: card.formatted_name.clone(),
                emails,
                note: card.note.clone(),
            })
            .await?;
        imported += 1;
    }
    Ok(VCardImportDto { imported, skipped })
}

/// Writes the whole address book to a vCard file, returning the number of
/// contacts exported.
///
/// # Errors
///
/// Returns [`MailError::Filesystem`] if the file cannot be written and
/// [`MailError::Database`] on any query failure.
pub async fn export_contacts_vcard_impl(
    store: &Store,
    path: String,
    version: Option<String>,
) -> Result<u32, MailError> {
    let contacts = store.contacts().list("", EXPORT_CAP).await?;
    let cards: Vec<VCard> = contacts
        .iter()
        .map(|contact| VCard {
            formatted_name: contact.display_name.clone(),
            emails: contact
                .emails
                .iter()
                .map(|email| VCardEmail {
                    address: email.email.as_str().to_owned(),
                    label: email.label.clone(),
                    preferred: email.is_primary,
                })
                .collect(),
            note: contact.note.clone(),
        })
        .collect();
    let count = u32::try_from(cards.len()).unwrap_or(u32::MAX);
    let document = emit_vcards(&cards, parse_version(version.as_deref()));
    write_file(path, document).await?;
    Ok(count)
}

/// Parses a contact's valid addresses out of a card, dropping malformed ones.
fn valid_emails(card: &VCard) -> Vec<NewContactEmail> {
    card.emails
        .iter()
        .filter_map(|email| {
            EmailAddress::parse(email.address.trim())
                .ok()
                .map(|address| NewContactEmail {
                    email: address,
                    label: email.label.clone(),
                    is_primary: email.preferred,
                })
        })
        .collect()
}

/// Maps a requested version string to a [`VCardVersion`] (default 3.0).
fn parse_version(version: Option<&str>) -> VCardVersion {
    match version.map(str::trim) {
        Some("4.0") | Some("4") => VCardVersion::V4,
        _ => VCardVersion::V3,
    }
}

/// Reads a text file off the blocking pool (`std::fs` is synchronous).
async fn read_file(path: String) -> Result<String, MailError> {
    tokio::task::spawn_blocking(move || std::fs::read_to_string(&path))
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("vCard read task failed: {error}"),
        })?
        .map_err(|error| MailError::Filesystem {
            message: format!("reading vCard file: {}", error.kind()),
        })
}

/// Writes a document to a file off the blocking pool.
async fn write_file(path: String, contents: String) -> Result<(), MailError> {
    tokio::task::spawn_blocking(move || std::fs::write(&path, contents))
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("vCard write task failed: {error}"),
        })?
        .map_err(|error| MailError::Filesystem {
            message: format!("writing vCard file: {}", error.kind()),
        })
}

/// Imports contacts from a vCard file.
#[tauri::command]
pub async fn import_contacts_vcard(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<VCardImportDto, String> {
    import_contacts_vcard_impl(state.store(), path)
        .await
        .map_err(map_command_error)
}

/// Exports the address book to a vCard file.
#[tauri::command]
pub async fn export_contacts_vcard(
    state: tauri::State<'_, AppState>,
    path: String,
    version: Option<String>,
) -> Result<u32, String> {
    export_contacts_vcard_impl(state.store(), path, version)
        .await
        .map_err(map_command_error)
}
