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
//! - no `javascript:`, `vbscript:`, or attacker-supplied `data:` URLs anywhere:
//!   `data:` is not in the scheme allowlist, so ammonia strips it from every
//!   attribute before the filter runs (proven against `<a href>` and non-`img`
//!   contexts by the adversarial corpus);
//! - hyperlinks keep only `http`/`https`/`mailto` targets (for display) and
//!   always carry `rel="noopener noreferrer"`; the frontend still intercepts
//!   activation — the retained `href` is for display only;
//! - inline `cid:` images resolve to an inline `data:` URI on `img@src` only
//!   when the caller supplies a matching message-local part (see
//!   [`sanitize_message_html_with_inline`] and [`InlineImages`]); a `cid:` with
//!   no supplied part is dropped, and without any inline map the `cid:` value is
//!   left in place for a later resolver. The emitted `data:` URI is the filter's
//!   trusted output, which ammonia does not re-scheme-check — that is why inline
//!   images work without `data:` being in the global scheme allowlist;
//! - remote `http(s)` images are blocked and counted by default, or kept when
//!   the caller passes an allowing [`RemoteContentPolicy`]; every `srcset` is
//!   removed and counted in [`SanitizedHtml::remote_blocked`] in *both* modes
//!   (tracking-pixel blocking).
//!
//! After sanitization a warning-only pass ([`crate::scam`]) walks the clean
//! DOM for deceptive-link tells; its findings ride along in
//! [`SanitizedHtml::scam_flags`] and never change the sanitized bytes.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::active_content::detect_active_content;
use crate::html_policy::message_policy;
use crate::inline_images::InlineImages;
use crate::remote_policy::RemoteContentPolicy;
use crate::scam::{ScamIndicator, detect_scam_indicators};

/// Result of sanitizing one HTML message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedHtml {
    /// The sanitized, allowlisted HTML fragment, safe for `iframe srcdoc`.
    pub html: String,
    /// Number of remote resources that were removed instead of loaded. With
    /// the default blocking policy this counts every remote `http(s)`/`data:`
    /// image and `srcset`; when remote images are allowed it counts only the
    /// resources still blocked in that mode (`data:` images and `srcset`).
    pub remote_blocked: usize,
    /// `true` when the input contained active content — scripts, forms,
    /// frames/embeds, or `on*` event handlers — that was stripped. A signal
    /// for the UI (for example, an "active content removed" badge); it is
    /// never a permission to render any of that content.
    pub had_active_content: bool,
    /// Deceptive-link indicators found in the sanitized DOM (warning only;
    /// empty for the common case). See [`crate::scam`].
    pub scam_flags: Vec<ScamIndicator>,
}

/// Sanitizes an HTML message body with the default (block-all) remote policy.
///
/// The transformation is deterministic and total: any `&str` yields a
/// [`SanitizedHtml`] whose `html` contains no scripts, event handlers,
/// forms, frames, embeds, remote resource loads, or dangerous-scheme URLs.
/// See the module documentation for the full guarantee list.
#[must_use]
pub fn sanitize_message_html(html: &str) -> SanitizedHtml {
    sanitize_message_html_with_policy(html, RemoteContentPolicy::block_all())
}

/// Sanitizes an HTML message body under an explicit [`RemoteContentPolicy`].
///
/// The only behavior the policy changes is whether remote `http(s)` `img@src`
/// targets are kept (and therefore not counted as blocked). Every other rule —
/// the tag/attribute allowlist, CSS and script removal, the link scheme
/// allowlist, and the blocking of `srcset` — is identical in both modes.
///
/// Inline `cid:` `img@src` values are left in place for a later resolver; use
/// [`sanitize_message_html_with_inline`] to embed them as `data:` URIs.
#[must_use]
pub fn sanitize_message_html_with_policy(html: &str, policy: RemoteContentPolicy) -> SanitizedHtml {
    sanitize_core(html, policy, None)
}

/// Sanitizes an HTML message body and embeds its inline `cid:` images.
///
/// Behaves exactly like [`sanitize_message_html_with_policy`], except a
/// `cid:` reference on `img@src` that matches a part in `inline` is rewritten
/// to that part's inline `data:` URI (see [`InlineImages`]). A `cid:` with no
/// matching part is dropped — it renders as a broken image and can neither
/// fetch nor exfiltrate. The `data:` URI is only ever emitted here, on
/// `img@src`, as the attribute filter's output; `data:` remains outside the
/// global scheme allowlist, so it is still rejected on `href` and every other
/// attribute.
#[must_use]
pub fn sanitize_message_html_with_inline(
    html: &str,
    policy: RemoteContentPolicy,
    inline: &InlineImages,
) -> SanitizedHtml {
    sanitize_core(html, policy, Some(Arc::new(inline.clone())))
}

/// Shared sanitization core. `inline` is `Some` only when the caller asked to
/// embed `cid:` images (the `Arc` lets the resolver ride into the `'static`
/// attribute-filter closure).
fn sanitize_core(
    html: &str,
    policy: RemoteContentPolicy,
    inline: Option<Arc<InlineImages>>,
) -> SanitizedHtml {
    let had_active_content = detect_active_content(html);

    let remote = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&remote);
    let allow_remote_images = policy.allow_remote_images;

    let mut builder = message_policy();
    builder.attribute_filter(move |element, attribute, value| {
        filter_attribute(
            element,
            attribute,
            value,
            allow_remote_images,
            inline.as_deref(),
            &counter,
        )
    });

    let cleaned = builder.clean(html).to_string();
    let scam_flags = detect_scam_indicators(&cleaned);

    SanitizedHtml {
        html: cleaned,
        remote_blocked: remote.load(Ordering::Relaxed),
        had_active_content,
        scam_flags,
    }
}

/// Sanitizes raw bytes with the default (block-all) policy, converting them to
/// text lossily first.
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
///
/// `allow_remote_images` relaxes exactly one arm: remote `http(s)` image
/// sources are kept (and not counted) instead of blocked. `inline` supplies
/// the message-local parts a `cid:` `img@src` resolves to. `srcset` is stripped
/// and counted regardless. Attacker `data:` never reaches here — it is rejected
/// at the scheme gate that runs before this filter — so there is no `data:`
/// arm; the only `data:` this stage produces is the trusted inline-image URI it
/// substitutes for a resolved `cid:`.
fn filter_attribute<'v>(
    element: &str,
    attribute: &str,
    value: &'v str,
    allow_remote_images: bool,
    inline: Option<&InlineImages>,
    counter: &AtomicUsize,
) -> Option<Cow<'v, str>> {
    match (element, attribute) {
        // Images: resolve inline `cid:` sources to a message-local `data:` URI
        // when a matching part was supplied (dropping any `cid:` that points
        // outside the message). Remote `http(s)` sources are kept only when the
        // policy allows them (and then not counted, since they are loaded);
        // otherwise they are blocked and counted.
        ("img", "src") => match url_scheme(value).as_deref() {
            Some("cid") => resolve_cid_image(value, inline),
            Some("http" | "https") if allow_remote_images => Some(Cow::Borrowed(value)),
            Some("http" | "https") => {
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

/// Resolves a `cid:` `img@src` to its inline `data:` URI.
///
/// With an inline map, a matching message-local part becomes a `data:` URI and
/// anything else (a `cid:` pointing outside the message, an oversize or
/// disallowed part) is dropped — the image cannot fetch or exfiltrate. Without
/// a map (the plain sanitize entry points), the original `cid:` value is left
/// in place for a later resolver. The reference is the substring after the
/// first colon, trimmed; the scheme itself was already validated as `cid`.
fn resolve_cid_image<'v>(value: &'v str, inline: Option<&InlineImages>) -> Option<Cow<'v, str>> {
    match inline {
        Some(map) => {
            let reference = value.split_once(':').map(|(_, rest)| rest.trim())?;
            map.resolve(reference).map(|uri| Cow::Owned(uri.to_owned()))
        }
        None => Some(Cow::Borrowed(value)),
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

#[cfg(test)]
mod tests {
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
}
