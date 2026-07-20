//! Message parsing entry point and part extraction.

use axomail_core::{MailError, UtcTimestamp};
use mail_parser::{
    ContentType, HeaderValue, Message, MessageParser, MessagePart, MimeHeaders, PartType,
};

use crate::address::{addresses_from_raw, collect_addresses, first_address};
use crate::model::{ParsedAttachment, ParsedMessage};
use crate::preview::build_preview;

/// Parses a raw RFC 5322/MIME message into a [`ParsedMessage`].
///
/// Parsing is tolerant: `mail_parser` recovers from most malformations, so
/// only input it cannot interpret as a message at all maps to
/// [`MailError::Parse`]. Malformed individual addresses are dropped and
/// recorded in [`ParsedMessage::parse_warnings`] rather than failing the
/// whole message.
///
/// # Errors
///
/// Returns [`MailError::Parse`] when the byte stream cannot be parsed as a
/// message.
pub fn parse_message(bytes: &[u8]) -> Result<ParsedMessage, MailError> {
    let message = MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| MailError::Parse {
            message: "input is not a parseable MIME message".to_owned(),
        })?;

    let mut parse_warnings = Vec::new();
    let from = collect_addresses(message.from(), "from", &mut parse_warnings);
    let to = collect_addresses(message.to(), "to", &mut parse_warnings);
    let cc = collect_addresses(message.cc(), "cc", &mut parse_warnings);
    let reply_to = first_address(message.reply_to(), "reply-to", &mut parse_warnings);
    // `Disposition-Notification-To` is not typed by the default parser (typing
    // it drops MIME-body parsing), so its addresses come from the raw value.
    let disposition_notification_to = message
        .header_raw("Disposition-Notification-To")
        .map(addresses_from_raw)
        .unwrap_or_default();
    let mailing_list = detect_mailing_list(&message);

    let text_body = extract_text_body(&message);
    let html_body = extract_html_body(&message);
    let preview = build_preview(text_body.as_deref(), html_body.as_deref());
    let attachments = extract_attachments(&message);

    Ok(ParsedMessage {
        subject: message.subject().map(str::to_owned),
        from,
        to,
        cc,
        reply_to,
        date: extract_date(&message),
        message_id: message.message_id().map(str::to_owned),
        in_reply_to: collect_ids(message.in_reply_to()),
        references: collect_ids(message.references()),
        text_body,
        html_body,
        preview,
        attachments,
        disposition_notification_to,
        mailing_list,
        parse_warnings,
    })
}

/// `List-*` headers (RFC 2369/2919) whose mere presence marks the message as
/// list mail.
const MAILING_LIST_HEADERS: [&str; 5] = [
    "List-Id",
    "List-Post",
    "List-Unsubscribe",
    "List-Help",
    "List-Archive",
];

/// Detects automated/list mail: any RFC 2369/2919 `List-*` header, or a
/// `Precedence` of `list` or `bulk`.
fn detect_mailing_list(message: &Message<'_>) -> bool {
    if MAILING_LIST_HEADERS
        .iter()
        .any(|name| message.header_raw(*name).is_some())
    {
        return true;
    }
    message.header_raw("Precedence").is_some_and(|value| {
        let value = value.trim().to_ascii_lowercase();
        value == "list" || value == "bulk"
    })
}

/// Extracts the first genuine `text/plain` body, if present.
///
/// The parser lists an HTML part as a text body for HTML-only messages;
/// matching on [`PartType::Text`] keeps this to actual plain-text content.
fn extract_text_body(message: &Message<'_>) -> Option<String> {
    message.text_bodies().find_map(|part| match &part.body {
        PartType::Text(text) => Some(text.as_ref().to_owned()),
        _ => None,
    })
}

/// Extracts the first genuine `text/html` body, if present.
fn extract_html_body(message: &Message<'_>) -> Option<String> {
    message.html_bodies().find_map(|part| match &part.body {
        PartType::Html(html) => Some(html.as_ref().to_owned()),
        _ => None,
    })
}

/// Collects the message-ids of an id-list header (`In-Reply-To`,
/// `References`) in document order, without angle brackets.
///
/// `mail_parser` already strips the brackets and yields either a single
/// [`HeaderValue::Text`] or a [`HeaderValue::TextList`]; `as_text_list`
/// normalizes both. Blank tokens (from a malformed header) are dropped.
fn collect_ids(value: &HeaderValue<'_>) -> Vec<String> {
    value
        .as_text_list()
        .map(|ids| {
            ids.iter()
                .map(|id| id.trim().to_owned())
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Maps a valid `Date` header to a UTC timestamp; invalid or missing dates
/// yield `None`.
fn extract_date(message: &Message<'_>) -> Option<UtcTimestamp> {
    message
        .date()
        .filter(|date| date.is_valid())
        .map(|date| UtcTimestamp::from_unix_seconds(date.to_timestamp()))
}

/// Collects every attachment and inline part in document order.
fn extract_attachments(message: &Message<'_>) -> Vec<ParsedAttachment> {
    message.attachments().map(build_attachment).collect()
}

/// Converts one `mail_parser` part into a [`ParsedAttachment`].
fn build_attachment(part: &MessagePart<'_>) -> ParsedAttachment {
    let content_id = part
        .content_id()
        .map(strip_angle_brackets)
        .filter(|id| !id.is_empty());
    let bytes = part.contents().to_vec();
    ParsedAttachment {
        filename: part.attachment_name().map(str::to_owned),
        mime_type: mime_type_of(part),
        size: bytes.len(),
        inline: is_inline(part, content_id.is_some()),
        content_id,
        bytes,
    }
}

/// Decides whether a part is inline.
///
/// An explicit `Content-Disposition` wins; otherwise a part is treated as
/// inline when it carries a `Content-ID` (as `cid:`-referenced images do).
fn is_inline(part: &MessagePart<'_>, has_content_id: bool) -> bool {
    let disposition = part.content_disposition();
    let is_attachment = disposition.is_some_and(ContentType::is_attachment);
    let is_inline = disposition.is_some_and(ContentType::is_inline);
    is_inline || (!is_attachment && has_content_id)
}

/// Builds a lowercase `type/subtype` string, defaulting to
/// `application/octet-stream` when no Content-Type is declared.
fn mime_type_of(part: &MessagePart<'_>) -> String {
    match part.content_type() {
        Some(content_type) => format!(
            "{}/{}",
            content_type.ctype().to_ascii_lowercase(),
            content_type
                .subtype()
                .unwrap_or("octet-stream")
                .to_ascii_lowercase()
        ),
        None => "application/octet-stream".to_owned(),
    }
}

/// Removes a single pair of surrounding angle brackets from a `Content-ID`.
fn strip_angle_brackets(id: &str) -> String {
    let trimmed = id.trim();
    trimmed
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(trimmed)
        .to_owned()
}
