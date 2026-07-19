//! Parsed message model produced by [`parse_message`](crate::parse_message).

use axomail_core::{Address, ThreadingHeaders, UtcTimestamp};
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
    /// `In-Reply-To` message-ids in header order, without angle brackets.
    /// Usually one parent, but the header may carry several.
    pub in_reply_to: Vec<String>,
    /// `References` chain (oldest ancestor first), without angle brackets.
    pub references: Vec<String>,
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

impl ParsedMessage {
    /// Collects the message's threading identity (Message-ID, In-Reply-To,
    /// References) into a [`ThreadingHeaders`] for storage and thread
    /// assembly.
    #[must_use]
    pub fn threading_headers(&self) -> ThreadingHeaders {
        ThreadingHeaders {
            message_id: self.message_id.clone(),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
        }
    }

    /// Iterates the message's downloadable attachments — every part that is not
    /// inline — in document order.
    ///
    /// This is the single ordering shared by the detail DTO's attachment list
    /// and the save/open commands, so an attachment index chosen in the UI
    /// resolves to the same part on the backend. Inline parts (`cid:` images
    /// referenced from the body) are excluded: they render in place and are not
    /// offered as separate downloads.
    pub fn downloadable_attachments(&self) -> impl Iterator<Item = &ParsedAttachment> {
        self.attachments.iter().filter(|part| !part.inline)
    }
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
