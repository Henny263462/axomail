//! Value types for the tag repository: the tag identifier, a validated
//! presentation color, and the new/persisted/update row structs.

use axomail_core::{AccountId, Keyword, MailError};
use serde::{Deserialize, Serialize};

/// Identifier of a persisted [`Tag`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagId(i64);

impl TagId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for TagId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated tag presentation color: a `#rrggbb` hex triple.
///
/// Kept as a strong value type (never a bare `String`) so an invalid color can
/// never reach storage or the UI; `serde` deserialization routes through the
/// same validation via `try_from`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TagColor(String);

impl TagColor {
    /// Parses and validates a `#rrggbb` color, lowercasing the hex digits so
    /// equal colors compare equal.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when the input is not a `#` followed
    /// by exactly six ASCII hex digits.
    pub fn parse(input: &str) -> Result<Self, MailError> {
        let invalid = || MailError::Configuration {
            message: "invalid tag color: expected #rrggbb".to_owned(),
        };
        let hex = input.strip_prefix('#').ok_or_else(invalid)?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        Ok(Self(format!("#{}", hex.to_ascii_lowercase())))
    }

    /// Returns the `#rrggbb` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TagColor {
    type Error = MailError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<TagColor> for String {
    fn from(value: TagColor) -> Self {
        value.0
    }
}

impl std::fmt::Display for TagColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A tag to insert, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTag {
    /// Owning account (tags are per-account).
    pub account: AccountId,
    /// Canonical IMAP keyword this tag projects.
    pub keyword: Keyword,
    /// Human-readable label.
    pub display_name: String,
    /// Presentation color, or `None` for colorless.
    pub color: Option<TagColor>,
    /// Ordering position within the account's tag list.
    pub sort_order: i64,
}

/// The editable content of an existing tag (its keyword and account are fixed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagUpdate {
    /// New human-readable label.
    pub display_name: String,
    /// New presentation color, or `None` to clear it.
    pub color: Option<TagColor>,
}

/// A persisted tag row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Row identifier.
    pub id: TagId,
    /// Owning account.
    pub account: AccountId,
    /// Canonical IMAP keyword.
    pub keyword: Keyword,
    /// Human-readable label.
    pub display_name: String,
    /// Presentation color, or `None`.
    pub color: Option<TagColor>,
    /// Ordering position within the account's tag list.
    pub sort_order: i64,
}

/// Payload of an `add_keyword` / `remove_keyword` [`crate::PendingOp`].
///
/// The write path (tag/untag a message) enqueues a pending op naming the
/// keyword to add or remove on the server; the op-replay executor parses this
/// payload and issues an additive `STORE +FLAGS`/`-FLAGS` for that keyword, so
/// server keywords the client never mentioned are preserved. This is the
/// storage-level contract shared by the command that enqueues and the sync
/// executor that drains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeywordOpPayload {
    /// The keyword to add or remove on the server.
    pub keyword: Keyword,
}

impl KeywordOpPayload {
    /// Serializes this payload to the JSON stored in a pending op's
    /// `payload_json` column.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if serialization fails (it never
    /// should for a valid keyword).
    pub fn to_json(&self) -> Result<String, MailError> {
        serde_json::to_string(self).map_err(|_| MailError::Configuration {
            message: "could not serialize keyword op payload".to_owned(),
        })
    }

    /// Parses a payload from a pending op's `payload_json`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the JSON is malformed or does
    /// not carry a valid keyword.
    pub fn from_json(payload_json: &str) -> Result<Self, MailError> {
        serde_json::from_str(payload_json).map_err(|_| MailError::Configuration {
            message: "keyword op payload is not a valid keyword".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn color_accepts_and_normalizes_hex() {
        assert_eq!(TagColor::parse("#AABBCC").unwrap().as_str(), "#aabbcc");
        assert_eq!(TagColor::parse("#0f0f0f").unwrap().as_str(), "#0f0f0f");
    }

    #[test]
    fn color_rejects_malformed_input() {
        for input in ["aabbcc", "#abc", "#gggggg", "#aabbccdd", "", "#aabbc"] {
            assert!(TagColor::parse(input).is_err(), "should reject {input:?}");
        }
    }

    #[test]
    fn color_serde_validates_on_deserialize() {
        let ok: Result<TagColor, _> = serde_json::from_str("\"#123456\"");
        assert!(ok.is_ok());
        let bad: Result<TagColor, _> = serde_json::from_str("\"red\"");
        assert!(bad.is_err());
    }
}
