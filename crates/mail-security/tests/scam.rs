//! Deceptive-link ("scam") heuristics over sanitized message HTML (Track 1.6).
//!
//! Every case runs through the real sanitizer so the detection sees exactly
//! the DOM the reading pane renders. The pass is warning-only: it must never
//! alter the sanitized bytes, only annotate them.
#![allow(clippy::unwrap_used)]

use axomail_security::{ScamIndicatorKind, sanitize_message_html};

/// Convenience: the kinds flagged for `input`, in order.
fn kinds(input: &str) -> Vec<ScamIndicatorKind> {
    sanitize_message_html(input)
        .scam_flags
        .into_iter()
        .map(|flag| flag.kind)
        .collect()
}

#[test]
fn flags_link_text_that_names_a_different_registrable_domain() {
    assert_eq!(
        kinds("<a href=\"https://evil.example/login\">paypal.com</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
    assert_eq!(
        kinds("<a href=\"https://evil.example\">www.paypal.com</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
    // Visible text that is itself a full URL is compared too.
    assert_eq!(
        kinds("<a href=\"https://evil.example\">https://paypal.com/login</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
}

#[test]
fn sees_through_inline_markup_in_the_link_text() {
    assert_eq!(
        kinds("<a href=\"https://evil.example\"><b>paypal</b>.com</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
}

#[test]
fn does_not_flag_same_registrable_domain_or_subdomains() {
    assert!(kinds("<a href=\"https://example.com/path\">example.com</a>").is_empty());
    // A bare host in the text and a subdomain in the href share a registrable
    // domain, so this is not deceptive.
    assert!(kinds("<a href=\"https://www.example.com/x\">example.com</a>").is_empty());
    // Multi-level public suffix handled by the curated list.
    assert!(kinds("<a href=\"https://shop.example.co.uk\">example.co.uk</a>").is_empty());
}

#[test]
fn does_not_flag_prose_or_file_names_as_hosts() {
    assert!(kinds("<a href=\"https://evil.example\">Click here to verify</a>").is_empty());
    assert!(kinds("<a href=\"https://evil.example\">Log in</a>").is_empty());
    // A file name must not be mistaken for a host (`.pdf` is not a TLD).
    assert!(kinds("<a href=\"https://evil.example\">report.pdf</a>").is_empty());
    // A bare version string is not a host either.
    assert!(kinds("<a href=\"https://evil.example\">2.0</a>").is_empty());
}

#[test]
fn flags_raw_ipv4_and_ipv6_hosts() {
    assert_eq!(
        kinds("<a href=\"http://192.168.0.1/login\">Secure login</a>"),
        vec![ScamIndicatorKind::IpAddressHost]
    );
    let ipv6 = kinds("<a href=\"http://[2001:db8::1]/x\">open</a>");
    assert!(
        ipv6.contains(&ScamIndicatorKind::IpAddressHost),
        "expected an IP-host flag for an IPv6 literal, got {ipv6:?}"
    );
}

#[test]
fn a_single_anchor_can_raise_both_indicators() {
    let flags = kinds("<a href=\"http://8.8.8.8\">paypal.com</a>");
    assert!(flags.contains(&ScamIndicatorKind::IpAddressHost));
    assert!(flags.contains(&ScamIndicatorKind::DeceptiveLink));
    assert_eq!(flags.len(), 2);
}

#[test]
fn identical_indicators_are_deduplicated() {
    let flags = kinds(concat!(
        "<a href=\"http://10.0.0.5/a\">one</a>",
        "<a href=\"http://10.0.0.5/b\">two</a>",
    ));
    assert_eq!(flags, vec![ScamIndicatorKind::IpAddressHost]);
}

#[test]
fn mailto_and_removed_links_carry_no_host_and_are_not_flagged() {
    // mailto has no host to compare, even when the visible text looks like one.
    assert!(kinds("<a href=\"mailto:help@paypal.com\">paypal.com</a>").is_empty());
    // A javascript href is stripped entirely, leaving no href to evaluate.
    assert!(kinds("<a href=\"javascript:alert(1)\">paypal.com</a>").is_empty());
}

#[test]
fn detection_never_mutates_the_sanitized_html() {
    let input = "<p>hi</p><a href=\"http://192.168.0.1\">bank.example</a>";
    let flagged = sanitize_message_html(input);
    let plain = sanitize_message_html("<p>hi</p><a href=\"https://ok.example\">ok</a>");
    // The scam pass is additive: it does not touch the sanitized bytes, which
    // still contain the anchor exactly as the sanitizer produced it.
    assert!(flagged.html.contains("bank.example"));
    assert!(plain.scam_flags.is_empty());
}

#[test]
fn flags_a_host_embedded_in_prose_link_text() {
    // The whitespace-bypass fix: a mismatching host buried in surrounding text
    // is still detected, not only when it is the entire link text.
    assert_eq!(
        kinds("<a href=\"https://evil.example/login\">Please sign in at paypal.com now</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
}

#[test]
fn does_not_flag_prose_with_no_real_host_token() {
    // The conservative gate survives the embedded scan: a non-TLD dotted token
    // in prose must not be read as a host.
    assert!(
        kinds("<a href=\"https://evil.example\">read my essay.Really it is good</a>").is_empty()
    );
}

#[test]
fn suppresses_deceptive_link_for_known_esp_redirectors() {
    // A click-tracker legitimately shows the brand while linking through its own
    // domain: no deceptive-link warning for a curated redirector...
    assert!(
        kinds("<a href=\"https://click.list-manage.com/track/click?u=1\">shop.brand.com</a>")
            .is_empty()
    );
    assert!(kinds("<a href=\"https://url9.sendgrid.net/ls/click?x=1\">brand.com</a>").is_empty());
    // ...but the identical shape through an unlisted domain is still flagged.
    assert_eq!(
        kinds("<a href=\"https://evil.example/track\">shop.brand.com</a>"),
        vec![ScamIndicatorKind::DeceptiveLink]
    );
}

#[test]
fn benign_message_has_no_flags() {
    let input = concat!(
        "<p>Hello, please read the <a href=\"https://example.org/notes\">release notes</a>.</p>",
        "<img src=\"cid:logo\">",
    );
    assert!(sanitize_message_html(input).scam_flags.is_empty());
}
