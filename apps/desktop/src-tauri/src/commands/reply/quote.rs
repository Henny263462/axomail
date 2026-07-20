//! Quoting and body assembly for replies and forwards.
//!
//! The plain-text citation used by a reply is a `>`-prefixed copy of the source
//! body under an attribution line (`On <date>, <sender> wrote:`). An HTML-only
//! source is degraded to text first (via `axomail-parser`), so the quote is
//! always plain text — the composer's body is plain text (ADR: rich text is
//! track 3.5). A forward instead reproduces the original under a labelled
//! header block. [`assemble`] then positions the reply area and the signature
//! around that block according to the identity's composition preferences.

use axomail_core::Address;
use axomail_parser::{ParsedMessage, html_to_plain_text};
use axomail_storage::{ReplyPosition, SignaturePlacement};

/// Builds the reply citation: an attribution line followed by the `>`-quoted
/// source body.
pub(crate) fn reply_quote(parsed: &ParsedMessage) -> String {
    let attribution = attribution_line(parsed);
    let quoted = quote_lines(&source_text(parsed));
    if quoted.is_empty() {
        attribution
    } else {
        format!("{attribution}\n{quoted}")
    }
}

/// Builds the inline-forward block: a labelled header summary of the original
/// followed by its body, reproduced (not quoted) as Thunderbird does.
pub(crate) fn forward_block(parsed: &ParsedMessage) -> String {
    let mut lines = vec!["-------- Forwarded Message --------".to_owned()];
    lines.push(format!(
        "Subject: {}",
        parsed.subject.as_deref().unwrap_or("")
    ));
    if let Some(sender) = parsed.from.first() {
        lines.push(format!("From: {}", format_sender(sender)));
    }
    if let Some(date) = parsed.date {
        lines.push(format!("Date: {}", format_datetime(date.unix_seconds())));
    }
    if !parsed.to.is_empty() {
        lines.push(format!("To: {}", join_addresses(&parsed.to)));
    }
    let header = lines.join("\n");
    let body = source_text(parsed);
    if body.is_empty() {
        header
    } else {
        format!("{header}\n\n{body}")
    }
}

/// Builds the signature block (`-- ` delimiter + text), or an empty string when
/// the identity has no signature. Matches the plain-text delimiter the frontend
/// signature helper uses (RFC 3676 §4.3).
pub(crate) fn signature_block(signature_text: Option<&str>) -> String {
    match signature_text {
        Some(text) if !text.trim().is_empty() => format!("-- \n{text}"),
        _ => String::new(),
    }
}

/// Assembles the composed body: the (empty) reply area where the user types,
/// the citation/forward block, and the signature, ordered by the identity's
/// reply position and signature placement. Empty segments are dropped so a
/// forward-as-attachment (no block) or a signatureless identity is clean.
pub(crate) fn assemble(
    reply_position: ReplyPosition,
    signature_placement: SignaturePlacement,
    block: &str,
    signature: &str,
) -> String {
    let mut middle: Vec<&str> = Vec::new();
    if signature_placement == SignaturePlacement::AboveQuote && !signature.is_empty() {
        middle.push(signature);
    }
    if !block.is_empty() {
        middle.push(block);
    }
    if signature_placement == SignaturePlacement::BelowQuote && !signature.is_empty() {
        middle.push(signature);
    }

    // The reply area is an empty segment; joining with a blank line leaves the
    // user a place to type above (top-posting) or below (bottom-posting).
    let mut segments: Vec<&str> = Vec::new();
    match reply_position {
        ReplyPosition::Above => {
            segments.push("");
            segments.extend(&middle);
        }
        ReplyPosition::Below => {
            segments.extend(&middle);
            segments.push("");
        }
    }
    segments.join("\n\n")
}

/// The attribution line introducing a quote. Falls back to a date-less form
/// when the source has no parseable Date header.
fn attribution_line(parsed: &ParsedMessage) -> String {
    let sender = parsed
        .from
        .first()
        .map_or_else(|| "an unknown sender".to_owned(), format_sender);
    match parsed.date {
        Some(date) => format!(
            "On {}, {sender} wrote:",
            format_datetime(date.unix_seconds())
        ),
        None => format!("{sender} wrote:"),
    }
}

/// The best plain-text rendering of a source body: its `text/plain` part, or a
/// tag-stripped degrade of its HTML part, or empty.
fn source_text(parsed: &ParsedMessage) -> String {
    if let Some(text) = parsed.text_body.as_deref()
        && !text.trim().is_empty()
    {
        return text.to_owned();
    }
    if let Some(html) = parsed.html_body.as_deref() {
        return html_to_plain_text(html);
    }
    String::new()
}

/// Prefixes every line with `> ` (blank lines become a bare `>`), the standard
/// plain-text quoting convention. Trailing whitespace on the input is trimmed
/// so the quote does not end in a run of empty `>` lines.
fn quote_lines(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split('\n')
        .map(|line| {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats an address as `Name <email>` when it has a display name, else just
/// its email.
fn format_sender(address: &Address) -> String {
    match &address.display_name {
        Some(name) if !name.trim().is_empty() => {
            format!("{name} <{}>", address.email.as_str())
        }
        _ => address.email.as_str().to_owned(),
    }
}

/// Joins addresses into a comma-separated `Name <email>` list.
fn join_addresses(addresses: &[Address]) -> String {
    addresses
        .iter()
        .map(format_sender)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Formats a Unix timestamp as `YYYY-MM-DD HH:MM UTC` without a date-library
/// dependency (the inverse of the civil-day math the IMAP/POP3 date parsers
/// already use).
fn format_datetime(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Converts a count of days since the Unix epoch into a `(year, month, day)`
/// civil date (proleptic Gregorian, UTC). Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axomail_core::{EmailAddress, UtcTimestamp};

    use super::*;

    fn addr(name: Option<&str>, email: &str) -> Address {
        Address {
            display_name: name.map(str::to_owned),
            email: EmailAddress::parse(email).unwrap(),
        }
    }

    fn message(text: Option<&str>, html: Option<&str>) -> ParsedMessage {
        ParsedMessage {
            subject: Some("Weekly sync".to_owned()),
            from: vec![addr(Some("Ada Lovelace"), "ada@example.org")],
            to: vec![addr(None, "team@example.org")],
            cc: Vec::new(),
            reply_to: None,
            // 2020-09-13 12:26:40 UTC
            date: Some(UtcTimestamp::from_unix_seconds(1_600_000_000)),
            message_id: Some("m1@example.org".to_owned()),
            in_reply_to: Vec::new(),
            references: Vec::new(),
            text_body: text.map(str::to_owned),
            html_body: html.map(str::to_owned),
            preview: String::new(),
            attachments: Vec::new(),
            disposition_notification_to: Vec::new(),
            mailing_list: false,
            parse_warnings: Vec::new(),
        }
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(format_datetime(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_datetime(1_600_000_000), "2020-09-13 12:26 UTC");
    }

    #[test]
    fn reply_quote_prefixes_plain_text_lines_under_an_attribution() {
        let quote = reply_quote(&message(Some("Hello\n\nWorld"), None));
        assert!(
            quote.starts_with("On 2020-09-13 12:26 UTC, Ada Lovelace <ada@example.org> wrote:")
        );
        assert!(quote.contains("\n> Hello"));
        assert!(
            quote.contains("\n>\n"),
            "blank source lines become a bare >"
        );
        assert!(quote.contains("\n> World"));
    }

    #[test]
    fn reply_quote_degrades_html_when_there_is_no_text_part() {
        let quote = reply_quote(&message(None, Some("<p>Hi <b>there</b></p>")));
        assert!(quote.contains("> Hi"));
        assert!(quote.contains("there"));
        // The markup is stripped from the quoted body (the attribution line
        // still carries the sender's `<email>` angle brackets, so a bare `<`
        // check would be wrong).
        assert!(!quote.contains("<b>"));
        assert!(!quote.contains("<p>"));
    }

    #[test]
    fn attribution_without_a_date_omits_the_date() {
        let mut msg = message(Some("Body"), None);
        msg.date = None;
        let quote = reply_quote(&msg);
        assert!(quote.starts_with("Ada Lovelace <ada@example.org> wrote:"));
    }

    #[test]
    fn forward_block_reproduces_headers_and_body() {
        let block = forward_block(&message(Some("Original body."), None));
        assert!(block.contains("-------- Forwarded Message --------"));
        assert!(block.contains("Subject: Weekly sync"));
        assert!(block.contains("From: Ada Lovelace <ada@example.org>"));
        assert!(block.contains("To: team@example.org"));
        assert!(block.contains("Original body."));
        // A forward reproduces, it does not `>`-quote.
        assert!(!block.contains("> Original"));
    }

    #[test]
    fn assemble_positions_reply_and_signature() {
        let sig = signature_block(Some("Ada"));
        assert_eq!(sig, "-- \nAda");

        // Top-posting, signature below the quote: reply area, then quote, then
        // signature at the very bottom.
        let body = assemble(
            ReplyPosition::Above,
            SignaturePlacement::BelowQuote,
            "QUOTE",
            &sig,
        );
        assert_eq!(body, "\n\nQUOTE\n\n-- \nAda");

        // Bottom-posting, signature above the quote: signature, quote, then the
        // reply area at the end.
        let body = assemble(
            ReplyPosition::Below,
            SignaturePlacement::AboveQuote,
            "QUOTE",
            &sig,
        );
        assert_eq!(body, "-- \nAda\n\nQUOTE\n\n");
    }

    #[test]
    fn assemble_drops_empty_block_and_signature() {
        // Forward-as-attachment: no block, no signature → an empty body.
        let body = assemble(ReplyPosition::Above, SignaturePlacement::BelowQuote, "", "");
        assert_eq!(body, "");
    }

    #[test]
    fn signature_block_is_empty_for_blank_signature() {
        assert_eq!(signature_block(None), "");
        assert_eq!(signature_block(Some("   ")), "");
    }
}
