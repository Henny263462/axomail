//! Lightweight "did this message carry active content?" scanner.
//!
//! [`detect_active_content`] is a single-pass heuristic used only to set the UI
//! signal [`crate::SanitizedHtml::had_active_content`] (for an "active content
//! removed" badge). The actual security guarantee is enforced by the ammonia
//! policy in [`crate::html`], not by this scan, so an occasional false positive
//! or negative here is harmless.

/// Tag names whose presence signals stripped active or embedding content.
const ACTIVE_TAGS: [&str; 8] = [
    "script", "form", "iframe", "object", "embed", "applet", "frame", "frameset",
];

/// Detects whether the raw input carried active content that sanitization
/// removes: any [`ACTIVE_TAGS`] element or any `on*` event-handler attribute.
pub(crate) fn detect_active_content(input: &str) -> bool {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Parse an optional closing slash, then an ASCII tag name.
        let mut j = i + 1;
        if j < len && bytes[j] == b'/' {
            j += 1;
        }
        let name_start = j;
        while j < len && bytes[j].is_ascii_alphanumeric() {
            j += 1;
        }
        // `name_start..j` spans only ASCII bytes, so slicing is valid here.
        if is_active_tag(&input[name_start..j]) || tag_has_event_handler(bytes, j) {
            return true;
        }
        i = j.max(i + 1);
    }
    false
}

/// Returns `true` if `name` (any case) is an active-content tag.
fn is_active_tag(name: &str) -> bool {
    ACTIVE_TAGS.iter().any(|tag| name.eq_ignore_ascii_case(tag))
}

/// Scans one tag's attribute region, starting just after the tag name,
/// for an `on*=` event-handler attribute. Stops at the tag's `>`.
fn tag_has_event_handler(bytes: &[u8], mut i: usize) -> bool {
    let len = bytes.len();
    while i < len {
        match bytes[i] {
            b'>' => return false,
            b if is_attr_boundary(b) => {
                if starts_event_handler(bytes, i + 1) {
                    return true;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// Bytes that can precede an attribute name inside a tag. Quotes are
/// included because `<a href="x"onmouseover=...>` needs no whitespace.
fn is_attr_boundary(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'`' | b'/')
}

/// Returns `true` if an `on` + letter(s) + `=` event handler begins at `p`.
fn starts_event_handler(bytes: &[u8], p: usize) -> bool {
    let len = bytes.len();
    if p + 2 >= len
        || !bytes[p].eq_ignore_ascii_case(&b'o')
        || !bytes[p + 1].eq_ignore_ascii_case(&b'n')
    {
        return false;
    }
    let mut q = p + 2;
    if !bytes[q].is_ascii_alphabetic() {
        return false; // Bare "on" is not a handler; require "on<letter>".
    }
    while q < len && bytes[q].is_ascii_alphabetic() {
        q += 1;
    }
    while q < len && bytes[q].is_ascii_whitespace() {
        q += 1;
    }
    q < len && bytes[q] == b'='
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_event_handlers_and_active_tags() {
        assert!(detect_active_content("<img onerror=alert(1)>"));
        assert!(detect_active_content("<a href=\"x\"onmouseover=y()>"));
        assert!(detect_active_content("<SCRIPT>bad()</SCRIPT>"));
        assert!(detect_active_content("<svg onload=alert(1)>"));
        assert!(detect_active_content("<form><input></form>"));
    }

    #[test]
    fn ignores_benign_content_in_detection() {
        assert!(!detect_active_content("<p>hello <b>world</b></p>"));
        assert!(!detect_active_content("5 < 10 and 10 > 5"));
        assert!(!detect_active_content("<a href=\"https://x\">link</a>"));
        // "on" not followed by a letter+`=` is not a handler.
        assert!(!detect_active_content("<div data-on=1>"));
    }
}
