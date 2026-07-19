//! Policy matrix for the remote-content allow mode (ADR-0007, Track 1.6).
//!
//! These tests prove that allowing remote images relaxes *exactly one thing* —
//! remote `http(s)` `img@src` — and nothing else: `data:` images are rejected
//! at the scheme gate, `srcset` stays blocked and counted, and every
//! scripting/CSS/scheme guard is untouched. `data:` is not in the scheme
//! allowlist, so it never reaches the counting filter; the `remote_blocked`
//! tally covers the resources that would otherwise trigger a network fetch
//! (remote `http(s)` images and `srcset`).
#![allow(clippy::unwrap_used)]

use axomail_security::{
    RemoteContentPolicy, sanitize_message_html, sanitize_message_html_with_policy,
};

/// Substrings that must never survive, whatever the remote-content policy is.
const FORBIDDEN: &[&str] = &[
    "<script",
    "onerror=",
    "onload=",
    "javascript:",
    "vbscript:",
    "<form",
    "<iframe",
    " style=",
    "srcset=",
    "data:image",
];

fn assert_invariants(out: &str) {
    let lower = out.to_ascii_lowercase();
    for needle in FORBIDDEN {
        assert!(
            !lower.contains(needle),
            "forbidden {needle:?} survived: {out:?}"
        );
    }
}

#[test]
fn block_mode_matches_the_default_entry_point() {
    let input = "<img src=\"https://tracker.example/p.gif\">";
    let default = sanitize_message_html(input);
    let explicit = sanitize_message_html_with_policy(input, RemoteContentPolicy::block_all());
    assert_eq!(default.html, explicit.html);
    assert_eq!(default.remote_blocked, explicit.remote_blocked);
    assert_eq!(explicit.remote_blocked, 1);
    assert!(!explicit.html.contains("https://tracker"));
}

#[test]
fn allow_mode_keeps_remote_http_images_and_does_not_count_them() {
    let input = "<img src=\"https://cdn.example/logo.png\" alt=\"logo\">";
    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert!(
        allowed.html.contains("https://cdn.example/logo.png"),
        "allowed remote image must be kept: {:?}",
        allowed.html
    );
    assert_eq!(allowed.remote_blocked, 0, "a loaded image is not blocked");
}

#[test]
fn allow_mode_still_rejects_data_images_at_the_scheme_gate() {
    // Adversarial: allowing remote images must NOT admit `data:` images. They
    // are stripped by the scheme allowlist before the counting filter runs, so
    // they are absent from the output and not tallied as a suppressed fetch.
    let input = "<img src=\"data:image/png;base64,AAAABBBB\">";
    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert_eq!(
        allowed.remote_blocked, 0,
        "data: never reaches the fetch tally"
    );
    assert!(!allowed.html.contains("data:image"));
    assert_invariants(&allowed.html);
}

#[test]
fn allow_mode_still_blocks_and_counts_srcset() {
    let input = "<img src=\"cid:keep\" srcset=\"https://a/1.png 1x, https://a/2.png 2x\">";
    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert_eq!(allowed.remote_blocked, 1, "srcset stays blocked");
    assert!(allowed.html.contains("cid:keep"));
    assert!(!allowed.html.contains("srcset"));
    assert!(!allowed.html.contains("https://a"));
}

#[test]
fn allow_mode_does_not_loosen_scripts_css_or_link_schemes() {
    // Adversarial: turning on remote images must leave every other guard as-is.
    let input = concat!(
        "<script>steal()</script>",
        "<div style=\"background:url(https://evil/leak)\">x</div>",
        "<a href=\"javascript:alert(1)\">bad</a>",
        "<a href=\"data:text/html,<b>no</b>\">bad2</a>",
        "<img src=\"data:image/gif;base64,QQ\">",
    );
    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert_invariants(&allowed.html);
    assert!(!allowed.html.contains("steal"));
    assert!(!allowed.html.contains("url("));
    assert!(!allowed.html.contains("alert"));
    assert!(allowed.html.contains('x'));
    assert!(
        allowed.had_active_content,
        "the script must still register as active content"
    );
}

#[test]
fn count_is_correct_for_a_mixed_document_in_both_modes() {
    let input = concat!(
        "<img src=\"https://a/remote.png\">", // remote http(s), a fetch
        "<img src=\"cid:inline\">",           // inline, never counted
        "<img src=\"data:image/png;base64,QQ\">", // data, dropped at the gate
        "<img src=\"cid:x\" srcset=\"https://a/2.png 2x\">", // srcset, a fetch
    );

    let blocked = sanitize_message_html(input);
    assert_eq!(
        blocked.remote_blocked, 2,
        "http + srcset are suppressed fetches; data: is dropped, not tallied"
    );

    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert_eq!(allowed.remote_blocked, 1, "only srcset stays blocked");
    assert!(allowed.html.contains("https://a/remote.png"));
    assert!(!allowed.html.contains("data:image"));
    assert!(!allowed.html.contains("https://a/2.png"));
}

#[test]
fn uppercase_data_scheme_is_still_blocked_when_images_allowed() {
    let input = "<img src=\"DATA:image/png;base64,QQ\">";
    let allowed = sanitize_message_html_with_policy(input, RemoteContentPolicy::allow_images());
    assert_eq!(
        allowed.remote_blocked, 0,
        "data: is rejected at the scheme gate"
    );
    assert!(!allowed.html.to_ascii_lowercase().contains("data:"));
}
