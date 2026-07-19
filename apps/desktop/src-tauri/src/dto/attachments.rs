//! DTOs for attachment and raw-message export commands (Track 3.6).
//!
//! No attachment bytes cross the IPC boundary in these results — the bytes are
//! written to disk by Rust `std::fs` against a dialog-chosen path (ADR-0010).
//! These types only report what was saved so the reading pane can confirm the
//! outcome.

use axomail_core::MailError;
use serde::Serialize;

use super::checked_id;

/// One attachment written to disk by a save/save-all command.
///
/// `filename` is the final on-disk name (sanitized and de-duplicated for
/// save-all); `bytes` is the number of bytes written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAttachmentDto {
    /// Final on-disk file name (single path component).
    pub filename: String,
    /// Number of bytes written to disk.
    pub bytes: i64,
}

impl SavedAttachmentDto {
    /// Builds a saved-attachment record, guarding the byte count against the
    /// JavaScript safe-integer range.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the byte count is out of range.
    pub fn new(filename: String, bytes: usize) -> Result<Self, MailError> {
        Ok(Self {
            filename,
            bytes: checked_id(i64::try_from(bytes).unwrap_or(i64::MAX))?,
        })
    }
}
