//! Identity DTOs: the per-account sender identities and the composition
//! preferences that ride with them.
//!
//! The preference enums are reused straight from `axomail_storage`, whose
//! serde spelling (`below_quote`, `above`, `inline`, `flat`, …) is the wire
//! contract the frontend's string unions mirror. Mailbox destinations are
//! carried as raw ids (Wave 3 consumes them) and are range-checked like every
//! other identifier that crosses to JavaScript.

use axomail_core::MailError;
use axomail_storage::{
    ArchiveGranularity, ForwardMode, Identity, ReplyPosition, SignaturePlacement,
};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// One sender identity, as listed by `list_identities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityDto {
    /// Local identifier.
    pub id: i64,
    /// Owning account.
    pub account_id: i64,
    /// From address.
    pub email: String,
    /// Display name shown in the From header; may be empty.
    pub display_name: String,
    /// Optional Reply-To address.
    pub reply_to: Option<String>,
    /// Optional plain-text signature (without the `-- ` delimiter).
    pub signature_text: Option<String>,
    /// Signature placement relative to a quote.
    pub signature_placement: SignaturePlacement,
    /// Reply body position relative to a quote.
    pub reply_position: ReplyPosition,
    /// Forward carrying mode.
    pub forward_mode: ForwardMode,
    /// Archive sub-folder granularity.
    pub archive_granularity: ArchiveGranularity,
    /// Whether this is the account's default identity.
    pub is_default: bool,
    /// Ordering position within the account's identity list.
    pub sort_position: i64,
    /// Mailbox sent copies are filed into (Wave 3), if chosen.
    pub sent_dest: Option<i64>,
    /// Mailbox drafts are filed into (Wave 3), if chosen.
    pub drafts_dest: Option<i64>,
    /// Mailbox templates are filed into (Wave 3), if chosen.
    pub templates_dest: Option<i64>,
    /// Mailbox archives are filed into (Wave 3), if chosen.
    pub archive_dest: Option<i64>,
}

impl IdentityDto {
    /// Builds an identity DTO from a stored record, validating its ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_domain(identity: Identity) -> Result<Self, MailError> {
        let f = identity.fields;
        Ok(Self {
            id: checked_id(identity.id.value())?,
            account_id: checked_id(identity.account.value())?,
            email: f.email,
            display_name: f.display_name,
            reply_to: f.reply_to,
            signature_text: f.signature_text,
            signature_placement: f.signature_placement,
            reply_position: f.reply_position,
            forward_mode: f.forward_mode,
            archive_granularity: f.archive_granularity,
            is_default: identity.is_default,
            sort_position: identity.sort_position,
            sent_dest: checked_opt(f.sent_dest.map(|m| m.value()))?,
            drafts_dest: checked_opt(f.drafts_dest.map(|m| m.value()))?,
            templates_dest: checked_opt(f.templates_dest.map(|m| m.value()))?,
            archive_dest: checked_opt(f.archive_dest.map(|m| m.value()))?,
        })
    }
}

/// Content submitted by the identity editor for a create or update.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInput {
    /// Account the identity belongs to (ignored on update; the row's account
    /// is fixed at creation).
    pub account_id: i64,
    /// From address; validated as an email at the command edge.
    pub email: String,
    /// Display name; may be empty.
    #[serde(default)]
    pub display_name: String,
    /// Optional Reply-To address; blank is treated as absent.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Optional plain-text signature; blank is treated as absent.
    #[serde(default)]
    pub signature_text: Option<String>,
    /// Signature placement.
    pub signature_placement: SignaturePlacement,
    /// Reply body position.
    pub reply_position: ReplyPosition,
    /// Forward carrying mode.
    pub forward_mode: ForwardMode,
    /// Archive sub-folder granularity.
    pub archive_granularity: ArchiveGranularity,
    /// Whether the identity should be (made) the account default.
    #[serde(default)]
    pub make_default: bool,
    /// Optional Sent destination mailbox id (Wave 3).
    #[serde(default)]
    pub sent_dest: Option<i64>,
    /// Optional Drafts destination mailbox id (Wave 3).
    #[serde(default)]
    pub drafts_dest: Option<i64>,
    /// Optional Templates destination mailbox id (Wave 3).
    #[serde(default)]
    pub templates_dest: Option<i64>,
    /// Optional Archive destination mailbox id (Wave 3).
    #[serde(default)]
    pub archive_dest: Option<i64>,
}

/// Range-checks an optional id crossing from JavaScript.
fn checked_opt(value: Option<i64>) -> Result<Option<i64>, MailError> {
    value.map(checked_id).transpose()
}
