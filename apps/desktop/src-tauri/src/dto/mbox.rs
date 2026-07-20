//! DTOs for the mbox export/import commands (Track 4.6).
//!
//! Only counts cross the IPC boundary — message bytes are read from and written
//! to disk by Rust against a dialog-chosen path (ADR-0010). Counts are routed
//! through [`checked_id`] for the same safe-integer guarantee as identifiers.

use axomail_core::MailError;
use serde::Serialize;

use super::checked_id;

/// Result of exporting a folder to an mbox file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MboxExportDto {
    /// Messages written to the mbox file.
    pub exported: i64,
    /// Messages skipped because their body could not be sourced (for example a
    /// remote message not cached while working offline).
    pub skipped: i64,
}

impl MboxExportDto {
    /// Builds an export result, guarding both counts against the JavaScript
    /// safe-integer range.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a count is out of range.
    pub fn new(exported: usize, skipped: usize) -> Result<Self, MailError> {
        Ok(Self {
            exported: checked_id(i64::try_from(exported).unwrap_or(i64::MAX))?,
            skipped: checked_id(i64::try_from(skipped).unwrap_or(i64::MAX))?,
        })
    }
}

/// Result of importing an mbox / `.eml` file into a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MboxImportDto {
    /// Messages ingested into the target folder.
    pub imported: i64,
}

impl MboxImportDto {
    /// Builds an import result, guarding the count against the JavaScript
    /// safe-integer range.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the count is out of range.
    pub fn new(imported: usize) -> Result<Self, MailError> {
        Ok(Self {
            imported: checked_id(i64::try_from(imported).unwrap_or(i64::MAX))?,
        })
    }
}
