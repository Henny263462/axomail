//! Composition-preference enums stored on an [`crate::Identity`].
//!
//! Each maps to a `CHECK`-constrained text column in migration `0008`. The
//! `as_str`/`parse` pairs are the single source of truth for the on-disk
//! spelling, so a stored value that no longer matches any variant surfaces as
//! a redacted [`MailError::Database`] rather than a silent default.

use axomail_core::MailError;
use serde::{Deserialize, Serialize};

/// Where a signature is placed relative to quoted text in a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignaturePlacement {
    /// Signature sits below the quoted original (bottom of the message).
    BelowQuote,
    /// Signature sits above the quoted original (right after the reply).
    AboveQuote,
}

impl SignaturePlacement {
    /// Serialized form stored in the `signature_placement` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BelowQuote => "below_quote",
            Self::AboveQuote => "above_quote",
        }
    }

    /// Parses a stored value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unrecognized value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "below_quote" => Ok(Self::BelowQuote),
            "above_quote" => Ok(Self::AboveQuote),
            _ => Err(unknown("signature placement")),
        }
    }
}

/// Whether a reply body starts above or below the quoted original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyPosition {
    /// Type the reply above the quote (top-posting).
    Above,
    /// Type the reply below the quote (bottom-posting).
    Below,
}

impl ReplyPosition {
    /// Serialized form stored in the `reply_position` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
        }
    }

    /// Parses a stored value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unrecognized value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "above" => Ok(Self::Above),
            "below" => Ok(Self::Below),
            _ => Err(unknown("reply position")),
        }
    }
}

/// How a forwarded message carries the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardMode {
    /// Original quoted inline in the body.
    Inline,
    /// Original attached as a `message/rfc822` part.
    Attachment,
}

impl ForwardMode {
    /// Serialized form stored in the `forward_mode` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Attachment => "attachment",
        }
    }

    /// Parses a stored value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unrecognized value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "inline" => Ok(Self::Inline),
            "attachment" => Ok(Self::Attachment),
            _ => Err(unknown("forward mode")),
        }
    }
}

/// Sub-folder granularity when filing into an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveGranularity {
    /// A single flat archive folder.
    Flat,
    /// One sub-folder per year.
    Yearly,
    /// One sub-folder per month.
    Monthly,
}

impl ArchiveGranularity {
    /// Serialized form stored in the `archive_granularity` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Yearly => "yearly",
            Self::Monthly => "monthly",
        }
    }

    /// Parses a stored value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unrecognized value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "flat" => Ok(Self::Flat),
            "yearly" => Ok(Self::Yearly),
            "monthly" => Ok(Self::Monthly),
            _ => Err(unknown("archive granularity")),
        }
    }
}

/// Builds the redacted decode error shared by every preference parser.
fn unknown(what: &str) -> MailError {
    MailError::Database {
        message: format!("read_identity: unknown {what}"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn preferences_round_trip_through_their_stored_form() {
        assert_eq!(
            SignaturePlacement::parse(SignaturePlacement::AboveQuote.as_str()).unwrap(),
            SignaturePlacement::AboveQuote
        );
        assert_eq!(
            ReplyPosition::parse(ReplyPosition::Above.as_str()).unwrap(),
            ReplyPosition::Above
        );
        assert_eq!(
            ForwardMode::parse(ForwardMode::Attachment.as_str()).unwrap(),
            ForwardMode::Attachment
        );
        assert_eq!(
            ArchiveGranularity::parse(ArchiveGranularity::Monthly.as_str()).unwrap(),
            ArchiveGranularity::Monthly
        );
    }

    #[test]
    fn unknown_stored_values_are_rejected() {
        assert!(SignaturePlacement::parse("sideways").is_err());
        assert!(ReplyPosition::parse("middle").is_err());
        assert!(ForwardMode::parse("teleport").is_err());
        assert!(ArchiveGranularity::parse("hourly").is_err());
    }
}
