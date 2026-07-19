//! Attachment and raw-message export commands (Track 3.6).
//!
//! These commands serve the reading pane's "save attachment", "save all",
//! "save as .eml", "view source", and "all headers" affordances. The bytes are
//! sourced in Rust from the cached raw `.eml` (fetched on demand when online,
//! via [`detail::load_raw_for_export`]) and written to a **dialog-chosen path**
//! with Rust `std::fs` — the webview never gains filesystem scope (ADR-0010).
//! Attacker-controlled attachment names are run through
//! [`sanitize_attachment_filename`] before they are joined to a save-all
//! directory, so no name can escape it.
//!
//! "Open with the OS handler" is intentionally **not** offered this wave: it
//! would require an `opener:allow-open-path` capability escalation and invites
//! the classic double-click-an-executable hazard. Saving is the security
//! forward MVP; opening is tracked as a follow-up.

use std::collections::HashSet;
use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError};
use axomail_parser::{ParsedMessage, parse_message};
use axomail_security::sanitize_attachment_filename;
use axomail_storage::Store;
use axomail_sync::ProviderFactory;

use crate::commands::detail::load_raw_for_export;
use crate::commands::map_command_error;
use crate::dto::SavedAttachmentDto;
use crate::state::AppState;

/// Loads and parses the raw message, sourcing the body on demand when online.
async fn load_parsed<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
) -> Result<ParsedMessage, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let bytes = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    parse_message(&bytes)
}

/// Writes `bytes` to `path`, mapping any IO error to a redacted
/// [`MailError::Filesystem`] (never the raw OS path or message).
async fn write_file(path: &Path, bytes: &[u8]) -> Result<usize, MailError> {
    tokio::fs::write(path, bytes)
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("writing to the chosen location: {}", error.kind()),
        })?;
    Ok(bytes.len())
}

/// Resolves a downloadable-attachment index, rejecting negatives and
/// out-of-range values with a configuration error.
fn attachment_at(
    parsed: &ParsedMessage,
    index: i64,
) -> Result<&axomail_parser::ParsedAttachment, MailError> {
    let index = usize::try_from(index).map_err(|_| MailError::Configuration {
        message: "invalid attachment index".to_owned(),
    })?;
    parsed
        .downloadable_attachments()
        .nth(index)
        .ok_or_else(|| MailError::Configuration {
            message: "the requested attachment does not exist".to_owned(),
        })
}

/// Saves one attachment's bytes to the dialog-chosen `dest` path.
///
/// The path already came from the OS save-file dialog (the consent gesture,
/// ADR-0010); the bytes are written there verbatim. The reported filename is
/// the chosen path's final component.
///
/// # Errors
///
/// Propagates sourcing, parse, and filesystem errors; returns
/// [`MailError::Configuration`] for an out-of-range attachment index.
pub async fn save_attachment_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
    attachment_index: i64,
    dest: &Path,
) -> Result<SavedAttachmentDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let parsed = load_parsed(store, factory, raw_root, offline, message_id).await?;
    let part = attachment_at(&parsed, attachment_index)?;
    let written = write_file(dest, &part.bytes).await?;
    let filename = dest
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "attachment".to_owned());
    SavedAttachmentDto::new(filename, written)
}

/// Saves every downloadable attachment into the dialog-chosen `dest_dir`.
///
/// Each attachment's server-supplied name is sanitized into a safe single path
/// component and de-duplicated against names already written in this call, so
/// no attachment can traverse out of the directory or silently overwrite a
/// sibling. Returns the records actually written (empty when the message has no
/// downloadable attachments).
///
/// # Errors
///
/// Propagates sourcing, parse, and filesystem errors.
pub async fn save_all_attachments_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
    dest_dir: &Path,
) -> Result<Vec<SavedAttachmentDto>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let parsed = load_parsed(store, factory, raw_root, offline, message_id).await?;
    let mut used: HashSet<String> = HashSet::new();
    let mut saved = Vec::new();
    for part in parsed.downloadable_attachments() {
        let base = sanitize_attachment_filename(part.filename.as_deref().unwrap_or_default());
        let name = deduplicate(&mut used, &base);
        let written = write_file(&dest_dir.join(&name), &part.bytes).await?;
        saved.push(SavedAttachmentDto::new(name, written)?);
    }
    Ok(saved)
}

/// Returns a variant of `name` not yet present in `used`, inserting it.
///
/// The first occurrence keeps `name`; later collisions get ` (n)` inserted
/// before the extension. De-duplication folds case with Unicode-aware
/// [`str::to_lowercase`] (not ASCII-only), so a case-only clash on a
/// case-insensitive filesystem — including non-ASCII variants like
/// `Übersicht.pdf` vs `übersicht.pdf` — still diverges instead of overwriting.
fn deduplicate(used: &mut HashSet<String>, name: &str) -> String {
    if used.insert(name.to_lowercase()) {
        return name.to_owned();
    }
    let (stem, ext) = split_extension(name);
    for n in 1.. {
        let candidate = format!("{stem} ({n}){ext}");
        if used.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("the counter yields a fresh candidate eventually")
}

/// Splits a filename into its stem and extension (including the leading dot),
/// treating a leading-dot name (`.env`) as all-stem.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

/// Returns the full raw RFC 5322 source of a message, decoded lossily to text
/// for a read-only "view source" pane.
///
/// # Errors
///
/// Propagates sourcing errors (unknown message, not cached while offline, ...).
pub async fn get_message_source_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
) -> Result<String, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let bytes = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Returns just the header block (everything before the first blank line) of a
/// message, for the reading pane's "all headers" toggle.
///
/// # Errors
///
/// Propagates sourcing errors.
pub async fn get_message_headers_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
) -> Result<String, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let bytes = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    Ok(String::from_utf8_lossy(header_block(&bytes)).into_owned())
}

/// Slices the RFC 5322 header block: everything before the first blank line
/// (`CRLF CRLF`, or a lone `LF LF` for tolerant inputs). The whole input is
/// treated as headers when no blank line is found.
fn header_block(bytes: &[u8]) -> &[u8] {
    if let Some(pos) = find(bytes, b"\r\n\r\n") {
        &bytes[..pos]
    } else if let Some(pos) = find(bytes, b"\n\n") {
        &bytes[..pos]
    } else {
        bytes
    }
}

/// First index of `needle` in `haystack`, or `None`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Saves the whole raw message to the dialog-chosen `dest` path as a `.eml`.
///
/// # Errors
///
/// Propagates sourcing and filesystem errors.
pub async fn save_message_eml_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    message_id: i64,
    dest: &Path,
) -> Result<i64, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let bytes = load_raw_for_export(store, factory, raw_root, message_id, offline).await?;
    let written = write_file(dest, &bytes).await?;
    crate::dto::checked_id(i64::try_from(written).unwrap_or(i64::MAX))
}

// --- Tauri command wrappers ------------------------------------------------

/// Saves one attachment to a path the user picked in the save-file dialog.
#[tauri::command]
pub async fn save_attachment(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    attachment_index: i64,
    dest_path: String,
) -> Result<SavedAttachmentDto, String> {
    save_attachment_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        message_id,
        attachment_index,
        Path::new(&dest_path),
    )
    .await
    .map_err(map_command_error)
}

/// Saves every downloadable attachment into a directory the user picked.
#[tauri::command]
pub async fn save_all_attachments(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    dest_dir: String,
) -> Result<Vec<SavedAttachmentDto>, String> {
    save_all_attachments_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        message_id,
        Path::new(&dest_dir),
    )
    .await
    .map_err(map_command_error)
}

/// Returns the raw `.eml` source of a message for the view-source pane.
#[tauri::command]
pub async fn get_message_source(
    state: tauri::State<'_, AppState>,
    message_id: i64,
) -> Result<String, String> {
    get_message_source_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        message_id,
    )
    .await
    .map_err(map_command_error)
}

/// Returns the raw header block of a message for the all-headers toggle.
#[tauri::command]
pub async fn get_message_headers(
    state: tauri::State<'_, AppState>,
    message_id: i64,
) -> Result<String, String> {
    get_message_headers_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        message_id,
    )
    .await
    .map_err(map_command_error)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::deduplicate;

    #[test]
    fn case_variants_including_non_ascii_are_not_overwritten() {
        let mut used = HashSet::new();
        // ASCII case-only clash: the second diverges.
        assert_eq!(deduplicate(&mut used, "Datei.pdf"), "Datei.pdf");
        assert_eq!(deduplicate(&mut used, "datei.pdf"), "datei (1).pdf");
        // Non-ASCII (umlaut) case-only clash — an ASCII-only fold would leave
        // Ü/ü distinct and silently overwrite on a case-insensitive filesystem.
        assert_eq!(deduplicate(&mut used, "Übersicht.pdf"), "Übersicht.pdf");
        assert_eq!(deduplicate(&mut used, "übersicht.pdf"), "übersicht (1).pdf");
    }
}

/// Saves the whole message as a `.eml` file to a dialog-chosen path.
#[tauri::command]
pub async fn save_message_eml(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    dest_path: String,
) -> Result<i64, String> {
    save_message_eml_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        message_id,
        Path::new(&dest_path),
    )
    .await
    .map_err(map_command_error)
}
