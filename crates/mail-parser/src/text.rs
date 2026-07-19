//! Plain-text reduction of HTML content.
//!
//! Unlike [`preview`](crate::preview), which collapses all whitespace into a
//! one-line summary, this preserves line structure so callers that quote a
//! message body (reply/forward citation) keep readable paragraph breaks. It is
//! *not* sanitization for display — that is the exclusive job of
//! `axomail-security` (ADR-0007). This only degrades markup to readable text.

use mail_parser::decoders::html::html_to_text;

/// Reduces an HTML fragment to plain text with `mail_parser`'s tolerant tag
/// stripper, preserving line breaks between block elements.
///
/// Used to build a quotable plain-text body from an HTML-only source message
/// when it carries no `text/plain` alternative.
#[must_use]
pub fn html_to_plain_text(html: &str) -> String {
    html_to_text(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_keeps_visible_words() {
        let out = html_to_plain_text("<p>Hello <b>there</b></p>");
        assert!(out.contains("Hello"));
        assert!(out.contains("there"));
        assert!(!out.contains('<'));
    }

    #[test]
    fn blank_html_reduces_to_blank_text() {
        assert!(html_to_plain_text("").trim().is_empty());
    }
}
