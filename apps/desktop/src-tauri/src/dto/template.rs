//! Message-template DTOs.
//!
//! A template is a reusable, pre-filled composition (recipients, subject,
//! body). Templates are stored as a JSON list in the `app_settings` KV table
//! rather than as messages in a Templates-role mailbox: the `mailboxes.role`
//! CHECK constraint (migration 0001) admits no new role value and this track
//! ships no migration, so a KV-backed library is the migration-free equivalent
//! (it also keeps templates out of the Drafts list). See `commands::templates`.

use serde::{Deserialize, Serialize};

/// A saved message template, as listed by `list_templates`.
///
/// Round-trips through the KV store (hence `Deserialize`) and crosses the IPC
/// boundary to the picker (hence `Serialize`). Recipients are stored as plain
/// address strings, matching the composer's own model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTemplateDto {
    /// Stable identifier, unique within the template library.
    pub id: i64,
    /// User-facing template name shown in the picker.
    pub name: String,
    /// Pre-filled subject.
    pub subject: String,
    /// Pre-filled primary recipients.
    pub to: Vec<String>,
    /// Pre-filled carbon-copy recipients.
    pub cc: Vec<String>,
    /// Pre-filled blind-carbon-copy recipients.
    pub bcc: Vec<String>,
    /// Pre-filled plain-text body.
    pub body_text: String,
}

/// Input for `save_template`: the composer's current content plus a name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTemplateInput {
    /// User-facing template name (required, trimmed).
    pub name: String,
    /// Subject to store.
    #[serde(default)]
    pub subject: String,
    /// Primary recipients to store.
    #[serde(default)]
    pub to: Vec<String>,
    /// Carbon-copy recipients to store.
    #[serde(default)]
    pub cc: Vec<String>,
    /// Blind-carbon-copy recipients to store.
    #[serde(default)]
    pub bcc: Vec<String>,
    /// Plain-text body to store.
    #[serde(default)]
    pub body_text: String,
}
