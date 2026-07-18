//! Parsed message model produced by [`parse_message`](crate::parse_message).

use axomail_core::{Address, UtcTimestamp};
use serde::{Deserialize, Serialize};

/// A fully parsed MIME message in axomail's internal representation.
///
/// Every field is already decoded (charset, transfer encoding, and RFC 2047
/// encoded words are resolved by the parser). Bodies are untrusted content:
/// `html_body` must be sanitized by `axomail-security` before display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMessage {
    /// Decoded `Subject`, if present.
    pub subject: Option<String>,
    /// Valid `From` addresses; malformed entries are dropped (see
    /// [`parse_warnings`](Self::parse_warnings)).
    pub from: Vec<Address>,
    /// Valid `To` addresses.
    pub to: Vec<Address>,
    /// Valid `Cc` addresses.
    pub cc: Vec<Address>,
    /// First valid `Reply-To` address, if any.
    pub reply_to: Option<Address>,
    /// `Date` header as a UTC timestamp; `None` when missing or invalid.
    pub date: Option<UtcTimestamp>,
    /// Decoded `Message-ID` without surrounding angle brackets, if present.
    pub message_id: Option<String>,
    /// Decoded `text/plain` body, if the message has one.
    pub text_body: Option<String>,
    /// Decoded `text/html` body, if the message has one. Untrusted markup.
    pub html_body: Option<String>,
    /// Short plain-text preview for list views (whitespace-collapsed,
    /// control characters stripped, capped in length). Empty when no body
    /// yields visible text.
    pub preview: String,
    /// Attachments and inline parts in document order.
    pub attachments: Vec<ParsedAttachment>,
    /// Redacted notes about content that was skipped (for example malformed
    /// addresses), so callers can surface a "some data could not be read"
    /// hint without failing the whole message.
    pub parse_warnings: Vec<String>,
}

/// A single attachment or inline part extracted from a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedAttachment {
    /// Decoded file name, if the part declares one.
    pub filename: Option<String>,
    /// Lowercased `type/subtype` MIME type, defaulting to
    /// `application/octet-stream` when the part omits a Content-Type.
    pub mime_type: String,
    /// Size of the decoded content in bytes (equal to `bytes.len()`).
    pub size: usize,
    /// Decoded `Content-ID` without angle brackets, used to resolve
    /// `cid:` references from inline HTML.
    pub content_id: Option<String>,
    /// Whether the part is inline (referenced from the body) rather than a
    /// downloadable attachment.
    pub inline: bool,
    /// Decoded raw content.
    pub bytes: Vec<u8>,
}
