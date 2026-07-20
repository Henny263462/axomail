//! Message-detail DTOs built from a parsed, sanitized message.

use axomail_core::{Address, MailError, MessageEnvelope, UtcTimestamp};
use axomail_parser::{ParsedAttachment, ParsedMessage};
use axomail_security::{
    InlineImagePart, InlineImages, RemoteContentPolicy, ScamIndicator,
    sanitize_message_html_with_inline,
};
use serde::Serialize;

use super::InvitationDto;
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

/// One deceptive-link ("scam") indicator surfaced to the reading pane.
///
/// Warning-only: the sanitized body is unchanged. `kind` is a stable slug
/// (`deceptiveLink` / `ipAddressHost`) the UI can style or localize; `detail`
/// is the secret-free, human-readable reason shown in the warning banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScamFlagDto {
    /// Stable machine-readable indicator kind.
    pub kind: String,
    /// Human-readable explanation for the warning banner.
    pub detail: String,
}

impl ScamFlagDto {
    /// Converts a security-layer indicator into its DTO form.
    fn from_indicator(indicator: &ScamIndicator) -> Self {
        Self {
            kind: indicator.kind.slug().to_owned(),
            detail: indicator.detail.clone(),
        }
    }
}

/// Full detail of one message, ready for the reading pane.
///
/// `sanitizedHtml` is the output of the sanitizer and is safe to hand to the
/// sandboxed `iframe srcdoc` renderer (ADR-0007). `remoteBlockedCount` and
/// `hadActiveContent` drive the "images blocked" / "active content removed"
/// badges; `remoteAllowed` reports whether remote images were kept for this
/// render (allow-once or an allowed sender); `scamFlags` carries any
/// deceptive-link warnings.
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
    /// Whether the message is flagged/starred (from local storage, not the
    /// parsed body): the authoritative state for the reading-pane star toggle.
    pub flagged: bool,
    /// Decoded plain-text body, if present.
    pub text_body: Option<String>,
    /// Sanitized HTML body, if the message had one.
    pub sanitized_html: Option<String>,
    /// Count of remote fetches suppressed during sanitization — blocked remote
    /// `http(s)` images and `srcset` (with remote images allowed, only
    /// `srcset`). `data:` URLs are rejected at the scheme gate and not counted.
    pub remote_blocked_count: i64,
    /// Whether remote `http(s)` images were kept for this render.
    pub remote_allowed: bool,
    /// Deceptive-link warnings found in the sanitized DOM (warning only).
    pub scam_flags: Vec<ScamFlagDto>,
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
    /// `true` only for the offline placeholder: the body is not cached and
    /// could not be fetched because the app is working offline. The header
    /// fields are still populated from the stored envelope; every body field
    /// is empty. The frontend renders an honest "not available offline" state.
    pub offline_unavailable: bool,
    /// The iMIP invitation card payload when the message carries a
    /// `text/calendar` invitation, else `None`.
    pub invitation: Option<InvitationDto>,
}

impl MessageDetailDto {
    /// Builds a detail DTO from a parsed message, sanitizing any HTML body
    /// under `policy` (which decides whether remote images are kept).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message id or an
    /// attachment size is out of the safe integer range.
    pub fn from_parsed(
        message_id: i64,
        parsed: &ParsedMessage,
        policy: RemoteContentPolicy,
        flagged: bool,
        invitation: Option<InvitationDto>,
    ) -> Result<Self, MailError> {
        // Embed message-local inline `cid:` images as `data:` URIs so a
        // `multipart/related` body renders its logo/signature images with no
        // network fetch (ADR-0007). The map is built from the parsed inline
        // parts; a `cid:` with no matching part is dropped by the sanitizer.
        let inline_images =
            InlineImages::from_parts(parsed.attachments.iter().filter_map(|part| {
                let content_id = part.content_id.as_deref()?;
                part.inline.then_some(InlineImagePart {
                    content_id,
                    mime_type: &part.mime_type,
                    bytes: &part.bytes,
                })
            }));
        let sanitized = parsed
            .html_body
            .as_deref()
            .map(|html| sanitize_message_html_with_inline(html, policy, &inline_images));
        let sender = parsed.from.first();
        // Downloadable attachments share the parser's `!inline` ordering with
        // the save/open commands, so a UI attachment index resolves to the same
        // part on the backend.
        let attachments = parsed
            .downloadable_attachments()
            .map(AttachmentMetaDto::from_parsed)
            .collect::<Result<Vec<_>, _>>()?;
        let scam_flags = sanitized.as_ref().map_or_else(Vec::new, |s| {
            s.scam_flags
                .iter()
                .map(ScamFlagDto::from_indicator)
                .collect()
        });
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
            flagged,
            text_body: parsed.text_body.clone(),
            remote_blocked_count: sanitized
                .as_ref()
                .map_or(0, |s| i64::try_from(s.remote_blocked).unwrap_or(i64::MAX)),
            remote_allowed: policy.allow_remote_images,
            scam_flags,
            had_active_content: sanitized.as_ref().is_some_and(|s| s.had_active_content),
            sanitized_html: sanitized.map(|s| s.html),
            attachments,
            message_id: parsed.message_id.clone(),
            in_reply_to: parsed.in_reply_to.clone(),
            references: parsed.references.clone(),
            offline_unavailable: false,
            invitation,
        })
    }

    /// Builds the offline placeholder for a message whose body is not cached
    /// and cannot be fetched while working offline. Carries the stored
    /// envelope header (sender, subject, date, star) so the reading pane still
    /// shows who/what/when, with `offline_unavailable` set and no body.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the message id is out of the
    /// safe integer range.
    pub fn offline_placeholder(
        message_id: i64,
        envelope: &MessageEnvelope,
    ) -> Result<Self, MailError> {
        let sender = envelope.from.first();
        Ok(Self {
            id: checked_id(message_id)?,
            subject: envelope.subject.clone(),
            from_name: sender
                .and_then(|address| address.display_name.clone())
                .unwrap_or_default(),
            from_address: sender
                .map(|address| address.email.as_str().to_owned())
                .unwrap_or_default(),
            to: envelope.to.iter().map(RecipientDto::from_address).collect(),
            cc: Vec::new(),
            date_unix: envelope.date.map(UtcTimestamp::unix_seconds),
            flagged: envelope.flags.flagged,
            text_body: None,
            sanitized_html: None,
            remote_blocked_count: 0,
            remote_allowed: false,
            scam_flags: Vec::new(),
            had_active_content: false,
            attachments: Vec::new(),
            message_id: None,
            in_reply_to: Vec::new(),
            references: Vec::new(),
            offline_unavailable: true,
            invitation: None,
        })
    }
}
