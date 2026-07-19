//! Message-detail DTOs built from a parsed, sanitized message.

use axomail_core::{Address, MailError};
use axomail_parser::{ParsedAttachment, ParsedMessage};
use axomail_security::sanitize_message_html;
use serde::Serialize;

use super::checked_id;

/// A recipient with an optional display name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientDto {
    /// Display name, if the header carried one.
    pub name: Option<String>,
    /// The address itself.
    pub address: String,
}

impl RecipientDto {
    /// Builds a recipient DTO from a parsed address.
    fn from_address(address: &Address) -> Self {
        Self {
            name: address.display_name.clone(),
            address: address.email.as_str().to_owned(),
        }
    }
}

/// Metadata for one downloadable attachment (no bytes cross the boundary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMetaDto {
    /// File name presented to the user (empty when the part declared none).
    pub filename: String,
    /// Lowercased MIME type.
    pub mime_type: String,
    /// Decoded size in bytes.
    pub size: i64,
}

impl AttachmentMetaDto {
    /// Builds attachment metadata from a parsed part.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the size cannot be
    /// represented in the JavaScript safe integer range.
    fn from_parsed(part: &ParsedAttachment) -> Result<Self, MailError> {
        Ok(Self {
            filename: part.filename.clone().unwrap_or_default(),
            mime_type: part.mime_type.clone(),
            size: checked_id(i64::try_from(part.size).unwrap_or(i64::MAX))?,
        })
    }
}

/// Full detail of one message, ready for the reading pane.
///
/// `sanitizedHtml` is the output of [`sanitize_message_html`] and is safe to
/// hand to the sandboxed `iframe srcdoc` renderer (ADR-0007); `remoteBlocked`
/// and `hadActiveContent` drive the "images blocked" / "active content
/// removed" badges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDetailDto {
    /// Local identifier.
    pub id: i64,
    /// Decoded subject, if any.
    pub subject: Option<String>,
    /// Display name of the first sender, or an empty string.
    pub from_name: String,
    /// Address of the first sender, or an empty string.
    pub from_address: String,
    /// Primary recipients.
    pub to: Vec<RecipientDto>,
    /// Carbon-copy recipients.
    pub cc: Vec<RecipientDto>,
    /// Date header in Unix seconds (UTC); `None` when absent.
    pub date_unix: Option<i64>,
    /// Decoded plain-text body, if present.
    pub text_body: Option<String>,
    /// Sanitized HTML body, if the message had one.
    pub sanitized_html: Option<String>,
    /// Count of remote resources blocked during sanitization.
    pub remote_blocked: i64,
    /// Whether active content (scripts, forms, ...) was stripped.
    pub had_active_content: bool,
    /// Downloadable attachment metadata (inline parts are excluded).
    pub attachments: Vec<AttachmentMetaDto>,
    /// `Message-ID` without angle brackets, if the message declared one.
    pub message_id: Option<String>,
    /// `In-Reply-To` message-ids in header order (usually one parent).
    pub in_reply_to: Vec<String>,
    /// `References` chain, oldest ancestor first.
    pub references: Vec<String>,
}

impl MessageDetailDto {
    /// Builds a detail DTO from a parsed message, sanitizing any HTML body.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message id or an
    /// attachment size is out of the safe integer range.
    pub fn from_parsed(message_id: i64, parsed: &ParsedMessage) -> Result<Self, MailError> {
        let sanitized = parsed.html_body.as_deref().map(sanitize_message_html);
        let sender = parsed.from.first();
        let attachments = parsed
            .attachments
            .iter()
            .filter(|part| !part.inline)
            .map(AttachmentMetaDto::from_parsed)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: checked_id(message_id)?,
            subject: parsed.subject.clone(),
            from_name: sender
                .and_then(|address| address.display_name.clone())
                .unwrap_or_default(),
            from_address: sender
                .map(|address| address.email.as_str().to_owned())
                .unwrap_or_default(),
            to: parsed.to.iter().map(RecipientDto::from_address).collect(),
            cc: parsed.cc.iter().map(RecipientDto::from_address).collect(),
            date_unix: parsed.date.map(axomail_core::UtcTimestamp::unix_seconds),
            text_body: parsed.text_body.clone(),
            remote_blocked: sanitized
                .as_ref()
                .map_or(0, |s| i64::try_from(s.remote_blocked).unwrap_or(i64::MAX)),
            had_active_content: sanitized.as_ref().is_some_and(|s| s.had_active_content),
            sanitized_html: sanitized.map(|s| s.html),
            attachments,
            message_id: parsed.message_id.clone(),
            in_reply_to: parsed.in_reply_to.clone(),
            references: parsed.references.clone(),
        })
    }
}
