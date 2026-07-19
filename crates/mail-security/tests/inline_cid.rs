//! Adversarial tests for inline `cid:` image embedding (ADR-0007, Track 3.6).
//!
//! The mechanism embeds a message-local `cid:` image as a `data:` URI on
//! `img@src` via the sanitizer's attribute filter, without adding `data:` to
//! the scheme allowlist. These tests prove the security envelope of that
//! rewrite: `data:` is emitted only for a resolved `cid:` on `img@src`, is
//! still rejected on `<a href>` and every other attribute, cannot be smuggled
//! in by the attacker directly, and a `cid:` pointing outside the message
//! resolves to nothing (no fetch, no exfiltration).
#![allow(clippy::unwrap_used)]

use axomail_security::{
    InlineImagePart, InlineImages, RemoteContentPolicy, sanitize_message_html_with_inline,
};

/// One inline PNG part with the given `Content-ID`.
fn png(cid: &str) -> InlineImages {
    InlineImages::from_parts([InlineImagePart {
        content_id: cid,
        mime_type: "image/png",
        bytes: b"\x89PNG\r\n\x1a\n",
    }])
}

#[test]
fn cid_image_resolves_to_a_data_uri_on_img_src() {
    let input = "<img src=\"cid:logo-1@example.org\" alt=\"logo\">";
    let out = sanitize_message_html_with_inline(
        input,
        RemoteContentPolicy::block_all(),
        &png("logo-1@example.org"),
    );
    assert!(
        out.html.contains("src=\"data:image/png;base64,"),
        "cid resolved to an inline data: URI: {:?}",
        out.html
    );
    assert!(out.html.contains("alt=\"logo\""));
    assert!(!out.html.contains("cid:"), "the cid: reference is gone");
    // An inline image is not a suppressed remote fetch.
    assert_eq!(out.remote_blocked, 0);
}

#[test]
fn data_uri_is_rejected_on_anchor_href_even_with_inline_resolution() {
    // Adversarial: the inline path must not create a `data:` sink on links.
    let input = concat!(
        "<a href=\"data:text/html,<script>alert(1)</script>\">click</a>",
        "<img src=\"cid:logo\">",
    );
    let out =
        sanitize_message_html_with_inline(input, RemoteContentPolicy::block_all(), &png("logo"));
    assert!(out.html.contains("click"), "link text is kept");
    assert!(
        !out.html.to_ascii_lowercase().contains("data:text/html"),
        "no data: href survives: {:?}",
        out.html
    );
    assert!(!out.html.contains("<script"));
    assert!(!out.html.contains("alert"));
    // The legitimate inline image still resolved.
    assert!(out.html.contains("data:image/png;base64,"));
}

#[test]
fn attacker_supplied_data_image_is_not_embedded() {
    // A hostile `data:` on `img@src` (not a `cid:`) is stripped at the scheme
    // gate; only message-local `cid:` parts become `data:` URIs.
    let input = "<img src=\"data:image/svg+xml,<svg onload=alert(1)>\">";
    let out = sanitize_message_html_with_inline(
        input,
        RemoteContentPolicy::block_all(),
        &InlineImages::default(),
    );
    assert!(!out.html.to_ascii_lowercase().contains("data:"));
    assert!(!out.html.contains("onload"));
    assert!(!out.html.contains("alert"));
}

#[test]
fn cid_pointing_outside_the_message_is_dropped_not_exfiltrated() {
    // The referenced content-id is not among the message's parts: it must
    // resolve to nothing (broken image), never a leak or a network reference.
    let input = "<img src=\"cid:not-in-this-message@evil\">";
    let out = sanitize_message_html_with_inline(
        input,
        RemoteContentPolicy::block_all(),
        &png("logo-1@example.org"),
    );
    assert!(!out.html.contains("cid:"), "unresolved cid is removed");
    assert!(!out.html.contains("data:"), "nothing is embedded for it");
    assert!(!out.html.contains("not-in-this-message"));
    // The <img> remains but with no src.
    assert!(out.html.contains("<img"));
}

#[test]
fn oversize_and_non_image_inline_parts_do_not_embed() {
    let big = vec![0u8; 2 * 1024 * 1024 + 1];
    let map = InlineImages::from_parts([
        InlineImagePart {
            content_id: "big",
            mime_type: "image/png",
            bytes: &big,
        },
        InlineImagePart {
            content_id: "doc",
            mime_type: "application/pdf",
            bytes: b"%PDF",
        },
    ]);
    let input = "<img src=\"cid:big\"><img src=\"cid:doc\">";
    let out = sanitize_message_html_with_inline(input, RemoteContentPolicy::block_all(), &map);
    assert!(
        !out.html.contains("data:"),
        "neither oversize nor non-image embeds"
    );
    assert!(!out.html.contains("cid:"));
}

#[test]
fn resolved_data_uri_only_appears_on_img_not_on_a_disallowed_attribute() {
    // A cid: on a link href is not an image sink and must be dropped, even
    // though the same content-id resolves for an <img>.
    let input = concat!("<a href=\"cid:logo\">link</a>", "<img src=\"cid:logo\">",);
    let out =
        sanitize_message_html_with_inline(input, RemoteContentPolicy::block_all(), &png("logo"));
    // The <img> got a data: URI; the <a> did not (its href was dropped).
    assert_eq!(out.html.matches("data:image/png;base64,").count(), 1);
    assert!(out.html.contains("link"), "link text preserved");
    assert!(!out.html.contains("href=\"cid"), "cid href dropped");
    assert!(!out.html.contains("href=\"data"), "no data: href created");
}

#[test]
fn inline_resolution_leaves_remote_images_blocked_and_counted() {
    let input = concat!(
        "<img src=\"cid:logo\">",
        "<img src=\"https://tracker.example/p.gif\">",
    );
    let out =
        sanitize_message_html_with_inline(input, RemoteContentPolicy::block_all(), &png("logo"));
    assert!(
        out.html.contains("data:image/png;base64,"),
        "inline image embedded"
    );
    assert!(
        !out.html.contains("tracker.example"),
        "remote image blocked"
    );
    assert_eq!(out.remote_blocked, 1, "the remote fetch is still counted");
}
