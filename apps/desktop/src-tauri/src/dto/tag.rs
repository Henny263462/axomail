//! Tag DTOs: the chip shown on list rows and the message detail, the full tag
//! record for the tag manager, and the per-message picker options.

use axomail_core::MailError;
use axomail_storage::Tag;
use serde::Serialize;

use super::checked_id;

/// A compact tag chip (colored dot + label) for list rows and the detail
/// header. Carries only what the chip renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagChipDto {
    /// Local identifier.
    pub id: i64,
    /// Human-readable label.
    pub display_name: String,
    /// Presentation color as `#rrggbb`, or `None` for colorless.
    pub color: Option<String>,
}

impl TagChipDto {
    /// Builds a chip from a stored tag, validating its id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the tag id is out of the safe
    /// integer range.
    pub fn from_tag(tag: &Tag) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(tag.id.value())?,
            display_name: tag.display_name.clone(),
            color: tag.color.as_ref().map(|c| c.as_str().to_owned()),
        })
    }
}

/// A full tag record for the tag manager, including its keyword and how many
/// messages carry it (drives the "in use" delete warning).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDto {
    /// Local identifier.
    pub id: i64,
    /// Owning account.
    pub account_id: i64,
    /// Canonical IMAP keyword this tag projects.
    pub keyword: String,
    /// Human-readable label.
    pub display_name: String,
    /// Presentation color as `#rrggbb`, or `None`.
    pub color: Option<String>,
    /// Display ordering within the account.
    pub sort_order: i64,
    /// Number of messages carrying this tag.
    pub usage_count: i64,
}

impl TagDto {
    /// Builds a tag DTO from a stored tag and its usage count.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_tag(tag: &Tag, usage_count: i64) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(tag.id.value())?,
            account_id: checked_id(tag.account.value())?,
            keyword: tag.keyword.as_str().to_owned(),
            display_name: tag.display_name.clone(),
            color: tag.color.as_ref().map(|c| c.as_str().to_owned()),
            sort_order: tag.sort_order,
            usage_count,
        })
    }
}

/// One option in the per-message tag picker: an account tag plus whether the
/// message currently carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTagOptionDto {
    /// Local identifier.
    pub id: i64,
    /// Human-readable label.
    pub display_name: String,
    /// Presentation color as `#rrggbb`, or `None`.
    pub color: Option<String>,
    /// Whether the message currently carries this tag.
    pub applied: bool,
}

/// The tag picker's payload for one message: its account (for inline
/// tag creation) and every account tag with its applied state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTagOptionsDto {
    /// Message the picker acts on.
    pub message_id: i64,
    /// Account whose tag set is offered.
    pub account_id: i64,
    /// Every account tag with its applied flag, in display order.
    pub options: Vec<MessageTagOptionDto>,
}
