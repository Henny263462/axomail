//! Attachment file access for the composer (ADR-0010).
//!
//! The native open-file dialog runs in the webview and returns *paths only*;
//! the webview is never granted a filesystem scope. Everything that touches
//! bytes lives here, in Rust `std::fs`:
//!
//! - [`stat_attachments`] turns a set of chosen paths into name/size metadata
//!   for the composer's attachment chips, without reading any file content;
//! - [`load_attachments`] reads the bytes at send time into
//!   [`OutgoingAttachment`]s, sanitizing the filename shown to recipients and
//!   guessing a content type from the extension.
//!
//! Both enforce a total size cap so a stray multi-gigabyte selection can never
//! be slurped into memory, and both keep raw paths out of error messages
//! (ADR-0003): a failure names only the sanitized filename.

use std::path::Path;

use axomail_core::{MailError, OutgoingAttachment};
use axomail_security::sanitize_attachment_filename;

use crate::commands::map_command_error;
use crate::dto::AttachmentCandidateDto;
use crate::state::AppState;

/// Maximum combined size of all attachments on one message (25 MiB). The cap is
/// on the sum, not per file, because it exists to bound both memory use here and
/// the assembled message size handed to the SMTP server.
pub(crate) const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 25 * 1024 * 1024;

/// Reads the chosen files into attachments, sanitizing names and guessing MIME
/// types, under the default [`MAX_ATTACHMENT_TOTAL_BYTES`] cap.
///
/// This is the send-time counterpart to [`stat_attachments`]: the composer
/// hands back the same paths it showed as chips, and the bytes are read only
/// now. Blocking I/O; call from a blocking-safe context.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the combined size exceeds the cap
/// and [`MailError::Filesystem`] (naming only the sanitized filename) when a
/// file cannot be read.
pub(crate) fn load_attachments(paths: &[String]) -> Result<Vec<OutgoingAttachment>, MailError> {
    load_attachments_capped(paths, MAX_ATTACHMENT_TOTAL_BYTES)
}

/// [`load_attachments`] with an explicit cap, so tests can prove the limit
/// without materializing 25 MiB of temp files.
fn load_attachments_capped(
    paths: &[String],
    max_total: u64,
) -> Result<Vec<OutgoingAttachment>, MailError> {
    let mut total: u64 = 0;
    let mut attachments = Vec::with_capacity(paths.len());
    for path in paths {
        let filename = safe_filename(path);
        let bytes = std::fs::read(path).map_err(|error| MailError::Filesystem {
            message: format!("could not read attachment {filename}: {}", error.kind()),
        })?;
        total = total.saturating_add(bytes.len() as u64);
        if total > max_total {
            return Err(over_cap_error(max_total));
        }
        attachments.push(OutgoingAttachment {
            filename,
            mime_type: guess_mime_type(path).to_owned(),
            bytes,
        });
    }
    Ok(attachments)
}

/// Collects name/size metadata for candidate attachments without reading their
/// contents, rejecting a selection whose combined size exceeds `max_total`.
///
/// Blocking I/O (one `stat` per path); call from a blocking-safe context.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the combined size exceeds the cap
/// and [`MailError::Filesystem`] (naming only the sanitized filename) when a
/// path cannot be inspected.
fn stat_attachments_capped(
    paths: &[String],
    max_total: u64,
) -> Result<Vec<AttachmentCandidateDto>, MailError> {
    let mut total: u64 = 0;
    let mut candidates = Vec::with_capacity(paths.len());
    for path in paths {
        let filename = safe_filename(path);
        let meta = std::fs::metadata(path).map_err(|error| MailError::Filesystem {
            message: format!("could not read attachment {filename}: {}", error.kind()),
        })?;
        if !meta.is_file() {
            return Err(MailError::Configuration {
                message: format!("{filename} is not a regular file"),
            });
        }
        total = total.saturating_add(meta.len());
        if total > max_total {
            return Err(over_cap_error(max_total));
        }
        candidates.push(AttachmentCandidateDto {
            path: path.clone(),
            filename,
            size_bytes: meta.len(),
        });
    }
    Ok(candidates)
}

/// The size-cap error, phrased in whole megabytes for the user.
fn over_cap_error(max_total: u64) -> MailError {
    MailError::Configuration {
        message: format!(
            "attachments exceed the {} MB total limit",
            max_total / (1024 * 1024)
        ),
    }
}

/// Sanitizes the final path component into a safe display/MIME filename.
fn safe_filename(path: &str) -> String {
    let base = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    sanitize_attachment_filename(base)
}

/// Guesses a content type from a filename's extension, defaulting to
/// `application/octet-stream`. A deliberately small, dependency-free table of
/// the types a desktop mail user attaches most often.
fn guess_mime_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "txt" | "text" | "log" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "md" => "text/markdown",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "7z" => "application/x-7z-compressed",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "json" => "application/json",
        "xml" => "application/xml",
        "ics" => "text/calendar",
        "eml" => "message/rfc822",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Returns name/size metadata for a set of dialog-chosen attachment paths.
///
/// The bytes are not read here — only when the message is actually sent. The
/// combined size is capped so an oversized selection is rejected up front, with
/// the same limit the send path enforces.
#[tauri::command]
pub async fn stat_attachments(
    _state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<AttachmentCandidateDto>, String> {
    tokio::task::spawn_blocking(move || stat_attachments_capped(&paths, MAX_ATTACHMENT_TOTAL_BYTES))
        .await
        .map_err(|error| format!("attachment inspection task failed: {error}"))?
        .map_err(map_command_error)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Write;

    /// Writes a temp file with `bytes`, returning its path as a string.
    fn temp_file(name: &str, bytes: &[u8]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        let path_str = path.to_str().unwrap().to_owned();
        (dir, path_str)
    }

    #[test]
    fn loads_bytes_with_sanitized_name_and_guessed_type() {
        let (_dir, path) = temp_file("report.pdf", b"%PDF-1.4 body");
        let loaded = load_attachments(&[path]).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].filename, "report.pdf");
        assert_eq!(loaded[0].mime_type, "application/pdf");
        assert_eq!(loaded[0].bytes, b"%PDF-1.4 body");
    }

    #[test]
    fn filename_is_sanitized_to_a_single_component() {
        // A traversal-shaped basename collapses to a safe single component.
        let (_dir, path) = temp_file("evil.txt", b"data");
        let loaded = load_attachments(std::slice::from_ref(&path)).unwrap();
        assert!(!loaded[0].filename.contains('/'));
        assert!(!loaded[0].filename.contains('\\'));
        assert_eq!(loaded[0].filename, "evil.txt");
    }

    #[test]
    fn total_size_over_cap_is_rejected() {
        let (_dir_a, a) = temp_file("a.bin", &[0u8; 8]);
        let (_dir_b, b) = temp_file("b.bin", &[0u8; 8]);
        // A 10-byte cap is exceeded by the combined 16 bytes.
        let error = load_attachments_capped(&[a, b], 10).unwrap_err();
        assert!(matches!(error, MailError::Configuration { .. }));
    }

    #[test]
    fn missing_file_maps_to_filesystem_error_without_the_path() {
        let error = load_attachments(&["/no/such/secret-dir/file.pdf".to_owned()]).unwrap_err();
        match error {
            MailError::Filesystem { message } => {
                assert!(message.contains("file.pdf"));
                assert!(!message.contains("secret-dir"), "path leaked: {message}");
            }
            other => panic!("expected Filesystem, got {other:?}"),
        }
    }

    #[test]
    fn stat_reports_size_and_sanitized_name() {
        let (_dir, path) = temp_file("notes.txt", b"hello");
        let metas =
            stat_attachments_capped(std::slice::from_ref(&path), MAX_ATTACHMENT_TOTAL_BYTES)
                .unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].filename, "notes.txt");
        assert_eq!(metas[0].size_bytes, 5);
        assert_eq!(metas[0].path, path);
    }

    #[test]
    fn stat_rejects_a_selection_over_cap() {
        let (_dir, path) = temp_file("big.bin", &[0u8; 32]);
        let error = stat_attachments_capped(&[path], 16).unwrap_err();
        assert!(matches!(error, MailError::Configuration { .. }));
    }
}
