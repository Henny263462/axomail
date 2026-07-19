//! Inline `cid:` image embedding for sanitized HTML mail (ADR-0007).
//!
//! A `multipart/related` message references its inline images from the HTML
//! body by `cid:` URL, carrying the bytes in sibling MIME parts. To display
//! them without any network fetch, the sanitizer rewrites a surviving
//! `cid:` `img@src` into a self-contained `data:` URI. This module owns the
//! trust decisions around that rewrite: which parts are eligible, the size
//! cap, the image-type allowlist, and the base64 `data:` encoding.
//!
//! The map is built once per message from the parsed parts and handed to
//! [`crate::sanitize_message_html_with_inline`]. A `cid:` reference that is not
//! in the map (an oversize part, a disallowed type, or a `cid:` pointing at
//! something outside the message) resolves to nothing and renders as a broken
//! image — it can neither fetch nor exfiltrate.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Maximum decoded size of a single inline image embedded as a `data:` URI.
///
/// Inline images are almost always small logos or signatures; the cap bounds
/// the memory and DOM cost of a hostile message that declares a huge inline
/// part, without affecting legitimate mail. An oversize part is skipped (its
/// `cid:` renders as a broken image) rather than truncated.
pub const MAX_INLINE_IMAGE_BYTES: usize = 2 * 1024 * 1024;

/// Raster image MIME types eligible for inline `data:` embedding.
///
/// SVG is deliberately excluded: it is foreign markup with a long history of
/// being an XSS and external-fetch vector, and ADR-0007's rule is "if in doubt,
/// strip". The type placed in the emitted `data:` URI is always one of these
/// literals, never an attacker-controlled string.
fn inlinable_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("image/png"),
        "image/jpeg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/bmp" => Some("image/bmp"),
        "image/avif" => Some("image/avif"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("image/x-icon"),
        _ => None,
    }
}

/// One candidate inline part offered to [`InlineImages::from_parts`].
///
/// Borrows the parser's decoded fields; nothing is copied until a part passes
/// every eligibility check.
#[derive(Debug, Clone, Copy)]
pub struct InlineImagePart<'a> {
    /// `Content-ID` with the angle brackets already stripped.
    pub content_id: &'a str,
    /// Lowercased `type/subtype` MIME type.
    pub mime_type: &'a str,
    /// Decoded content bytes.
    pub bytes: &'a [u8],
}

/// Pre-encoded inline-image `data:` URIs keyed by their `Content-ID`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineImages {
    by_content_id: HashMap<String, String>,
}

impl InlineImages {
    /// Builds the inline-image map from a message's parts.
    ///
    /// A part is embedded only when it has a non-empty `Content-ID`, an
    /// allowlisted image MIME type, and a decoded size within
    /// [`MAX_INLINE_IMAGE_BYTES`]; everything else is skipped. When two parts
    /// share a `Content-ID` the first wins.
    #[must_use]
    pub fn from_parts<'a, I>(parts: I) -> Self
    where
        I: IntoIterator<Item = InlineImagePart<'a>>,
    {
        let mut by_content_id: HashMap<String, String> = HashMap::new();
        for part in parts {
            if part.content_id.is_empty() || part.bytes.len() > MAX_INLINE_IMAGE_BYTES {
                continue;
            }
            let Some(mime) = inlinable_mime(part.mime_type) else {
                continue;
            };
            by_content_id
                .entry(part.content_id.to_owned())
                .or_insert_with(|| encode_data_uri(mime, part.bytes));
        }
        Self { by_content_id }
    }

    /// Resolves the `cid:` reference `content_id` (bracket-free, scheme
    /// stripped) to its pre-encoded `data:` URI, or `None` when the message
    /// carries no such inline image.
    #[must_use]
    pub fn resolve(&self, content_id: &str) -> Option<&str> {
        self.by_content_id.get(content_id).map(String::as_str)
    }

    /// Whether the map holds no embeddable inline images.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_content_id.is_empty()
    }

    /// Number of embeddable inline images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_content_id.len()
    }
}

/// Encodes `bytes` as a base64 `data:` URI with the (allowlisted) `mime` type.
fn encode_data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", BASE64.encode(bytes))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn part<'a>(cid: &'a str, mime: &'a str, bytes: &'a [u8]) -> InlineImagePart<'a> {
        InlineImagePart {
            content_id: cid,
            mime_type: mime,
            bytes,
        }
    }

    #[test]
    fn embeds_allowlisted_image_as_base64_data_uri() {
        let images = InlineImages::from_parts([part("logo@x", "image/png", b"\x89PNG\r\n")]);
        let uri = images.resolve("logo@x").unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        assert_eq!(uri, encode_data_uri("image/png", b"\x89PNG\r\n"));
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn skips_disallowed_types_oversize_and_empty_ids() {
        let big = vec![0u8; MAX_INLINE_IMAGE_BYTES + 1];
        let images = InlineImages::from_parts([
            part("svg@x", "image/svg+xml", b"<svg/>"),
            part("pdf@x", "application/pdf", b"%PDF"),
            part("big@x", "image/png", &big),
            part("", "image/png", b"data"),
        ]);
        assert!(images.is_empty(), "no ineligible part is embedded");
    }

    #[test]
    fn first_writer_wins_on_duplicate_content_id() {
        let images = InlineImages::from_parts([
            part("dup@x", "image/gif", b"first"),
            part("dup@x", "image/gif", b"second"),
        ]);
        assert_eq!(
            images.resolve("dup@x").unwrap(),
            encode_data_uri("image/gif", b"first")
        );
    }

    #[test]
    fn icon_aliases_normalize_to_a_single_literal_type() {
        let images = InlineImages::from_parts([part("i@x", "image/vnd.microsoft.icon", b"ico")]);
        assert!(
            images
                .resolve("i@x")
                .unwrap()
                .starts_with("data:image/x-icon;base64,")
        );
    }
}
