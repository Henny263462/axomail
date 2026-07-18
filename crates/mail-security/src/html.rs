//! HTML sanitization for untrusted message bodies (ADR-0007).
//!
//! [`sanitize_message_html`] is layer one of the four-layer rendering
//! defense: it reduces arbitrary, hostile HTML to an allowlisted subset
//! (see [`crate::html_policy`]) that is safe to hand to the sandboxed
//! `iframe srcdoc` renderer. The function is pure, deterministic, and never
//! panics on any input; adversarial and garbage bytes are handled at the
//! call boundary via [`sanitize_message_html_bytes`].
//!
//! What it guarantees about the output:
//!
//! - no `<script>`, `<style>`, `<form>`/`<input>`, `<iframe>`, `<object>`,
//!   `<embed>`, `<applet>`, `<meta>`, `<base>`, `<link>`, `<svg>`/`<math>`,
//!   or other active/embedding elements;
//! - no `on*` event-handler attributes and no `style`/`class` attributes;
//! - no `javascript:`, `vbscript:`, or `data:` URLs anywhere;
//! - hyperlinks keep only `http`/`https`/`mailto` targets (for display) and
//!   always carry `rel="noopener noreferrer"`; the frontend still intercepts
//!   activation — the retained `href` is for display only;
//! - inline `cid:` images are preserved (resolved locally later); every
//!   remote `http(s)`/`data:` image or `srcset` is removed and counted in
//!   [`SanitizedHtml::remote_blocked`] (tracking-pixel blocking by default).

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::html_policy::message_policy;

/// Result of sanitizing one HTML message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedHtml {
    /// The sanitized, allowlisted HTML fragment, safe for `iframe srcdoc`.
    pub html: String,
    /// Number of remote resources (images / media / `srcset` / `data:`
    /// images) that were removed instead of loaded.
    pub remote_blocked: usize,
    /// `true` when the input contained active content — scripts, forms,
    /// frames/embeds, or `on*` event handlers — that was stripped. A signal
    /// for the UI (for example, an "active content removed" badge); it is
    /// never a permission to render any of that content.
    pub had_active_content: bool,
}

/// Sanitizes an HTML message body into an allowlisted, render-safe fragment.
///
/// The transformation is deterministic and total: any `&str` yields a
/// [`SanitizedHtml`] whose `html` contains no scripts, event handlers,
/// forms, frames, embeds, remote resource loads, or dangerous-scheme URLs.
/// See the module documentation for the full guarantee list.
#[must_use]
pub fn sanitize_message_html(html: &str) -> SanitizedHtml {
    let had_active_content = detect_active_content(html);

    let remote = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&remote);

    let mut builder = message_policy();
    builder.attribute_filter(move |element, attribute, value| {
        filter_attribute(element, attribute, value, &counter)
    });

    let cleaned = builder.clean(html).to_string();

    SanitizedHtml {
        html: cleaned,
        remote_blocked: remote.load(Ordering::Relaxed),
        had_active_content,
    }
}

/// Sanitizes raw bytes, converting them to text lossily first.
///
/// Message bodies arrive as bytes whose declared charset may be wrong or
/// whose content may be malformed; invalid UTF-8 is replaced with the
/// Unicode replacement character rather than rejected, so no byte sequence
/// can cause a failure at this boundary.
#[must_use]
pub fn sanitize_message_html_bytes(bytes: &[u8]) -> SanitizedHtml {
    sanitize_message_html(&String::from_utf8_lossy(bytes))
}

/// Per-attribute filter run by ammonia after its own allowlist and
/// scheme checks. Only attributes that already survived the policy reach
/// here; this stage enforces the resource and link rules and tallies
/// blocked remote content into `counter`.
fn filter_attribute<'v>(
    element: &str,
    attribute: &str,
    value: &'v str,
    counter: &AtomicUsize,
) -> Option<Cow<'v, str>> {
    match (element, attribute) {
        // Images: keep inline `cid:` sources; block and count remote or
        // `data:` sources (tracking-pixel defense).
        ("img", "src") => match url_scheme(value).as_deref() {
            Some("cid") => Some(Cow::Borrowed(value)),
            Some("http" | "https" | "data") => {
                counter.fetch_add(1, Ordering::Relaxed);
                None
            }
            _ => None,
        },
        // `srcset` only ever references remote candidates for our purposes;
        // ammonia does not scheme-check it, so drop and count it wholesale.
        ("img", "srcset") => {
            counter.fetch_add(1, Ordering::Relaxed);
            None
        }
        // Links: keep only web and mail targets, for display. Activation is
        // intercepted by the frontend; `cid:`/`data:` links are removed.
        ("a", "href") => match url_scheme(value).as_deref() {
            Some("http" | "https" | "mailto") => Some(Cow::Borrowed(value)),
            _ => None,
        },
        // Every other surviving attribute is expected to be presentational
        // and non-URL. As a defensive net against the allowlist growing a new
        // URL sink without a matching arm above, any URL-named attribute
        // carrying an active or non-web scheme is still dropped here. Text
        // attributes (`alt`, `title`, ...) are unaffected: the guard only
        // fires on recognized URL attribute names.
        _ => {
            if is_url_attribute_name(attribute)
                && matches!(
                    url_scheme(value).as_deref(),
                    Some("javascript" | "vbscript" | "data" | "cid")
                )
            {
                None
            } else {
                Some(Cow::Borrowed(value))
            }
        }
    }
}

/// Recognized URL-bearing attribute names, used only by the defensive guard
/// in [`filter_attribute`]. Not an allowlist: it never admits anything, it
/// only lets the guard recognize a URL sink that would otherwise slip
/// through the catch-all if the policy allowlist is extended in future.
fn is_url_attribute_name(attribute: &str) -> bool {
    matches!(
        attribute,
        "href"
            | "src"
            | "srcset"
            | "lowsrc"
            | "dynsrc"
            | "background"
            | "poster"
            | "cite"
            | "action"
            | "formaction"
            | "ping"
            | "longdesc"
            | "usemap"
            | "profile"
            | "manifest"
            | "codebase"
            | "data"
            | "xlink:href"
    )
}

/// Extracts the lowercased URL scheme of `value`, if it has a well-formed
/// one. Leading ASCII whitespace and control bytes (which user agents strip)
/// are ignored. Returns `None` for scheme-relative or relative references.
fn url_scheme(value: &str) -> Option<String> {
    let trimmed = value.trim_start_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    let colon = trimmed.find(':')?;
    let scheme = &trimmed[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        Some(scheme.to_ascii_lowercase())
    } else {
        None
    }
}

/// Tag names whose presence signals stripped active or embedding content.
const ACTIVE_TAGS: [&str; 8] = [
    "script", "form", "iframe", "object", "embed", "applet", "frame", "frameset",
];

/// Detects whether the raw input carried active content that sanitization
/// removes: any [`ACTIVE_TAGS`] element or any `on*` event-handler attribute.
///
/// This is a lightweight, single-pass heuristic used only to set the UI
/// signal [`SanitizedHtml::had_active_content`]; the actual security
/// guarantee is enforced by the ammonia policy, not by this scan, so an
/// occasional false positive or negative here is harmless.
fn detect_active_content(input: &str) -> bool {
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
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn url_scheme_parses_and_lowercases() {
        assert_eq!(url_scheme("HTTP://x").as_deref(), Some("http"));
        assert_eq!(url_scheme("  cid:abc").as_deref(), Some("cid"));
        assert_eq!(url_scheme("mailto:a@b").as_deref(), Some("mailto"));
        assert_eq!(url_scheme("/relative/path"), None);
        assert_eq!(url_scheme("noscheme"), None);
        assert_eq!(url_scheme("1http://x"), None);
    }

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
