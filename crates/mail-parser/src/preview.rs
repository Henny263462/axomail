//! Plain-text preview derivation for list views.
//!
//! Previews are intentionally lossy: runs of whitespace collapse to a single
//! space, control characters are dropped, and the result is capped in length.
//! HTML is reduced to text with `mail_parser`'s tolerant tag stripper — this
//! is *not* sanitization for display, which lives in `axomail-security`
//! (ADR-0007); it only produces readable summaries for message lists.

use mail_parser::decoders::html::html_to_text;

/// Upper bound on preview length in characters.
const MAX_PREVIEW_CHARS: usize = 160;

/// Builds a preview from the extracted bodies.
///
/// Prefers the plain-text body; falls back to tag-stripped HTML; yields an
/// empty string when neither produces visible text.
pub(crate) fn build_preview(text_body: Option<&str>, html_body: Option<&str>) -> String {
    if let Some(text) = text_body {
        let preview = normalize(text);
        if !preview.is_empty() {
            return preview;
        }
    }
    if let Some(html) = html_body {
        let preview = normalize(&html_to_text(html));
        if !preview.is_empty() {
            return preview;
        }
    }
    String::new()
}

/// Collapses whitespace, drops control characters, and truncates to
/// [`MAX_PREVIEW_CHARS`] characters.
fn normalize(input: &str) -> String {
    let mut collapsed = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.chars() {
        if ch.is_control() || ch.is_whitespace() {
            // Only remember a separator once real content has started, so
            // leading whitespace never produces a leading space.
            pending_space = !collapsed.is_empty();
            continue;
        }
        if pending_space {
            collapsed.push(' ');
            pending_space = false;
        }
        collapsed.push(ch);
    }
    truncate_chars(&collapsed, MAX_PREVIEW_CHARS)
}

/// Returns the first `max` characters of `input`, respecting char boundaries.
fn truncate_chars(input: &str, max: usize) -> String {
    input.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_drops_controls() {
        let out = build_preview(Some("  Hello\r\n\r\nworld  \tfoo\u{0}bar  "), None);
        assert_eq!(out, "Hello world foo bar");
    }

    #[test]
    fn truncates_to_the_character_cap() {
        let long = "A".repeat(300);
        let out = build_preview(Some(&long), None);
        assert_eq!(out.chars().count(), MAX_PREVIEW_CHARS);
    }

    #[test]
    fn falls_back_to_html_when_text_is_blank() {
        let out = build_preview(Some("   \n\t "), Some("<p>Hi <b>there</b></p>"));
        assert!(out.contains("Hi"));
        assert!(out.contains("there"));
        assert!(!out.contains('<'));
    }

    #[test]
    fn empty_when_no_body_has_visible_text() {
        assert_eq!(build_preview(None, None), "");
        assert_eq!(build_preview(Some(""), None), "");
    }
}
