//! Tests that redaction never lets a full address or token survive.
#![allow(clippy::unwrap_used)]

use axomail_security::{redact_email, redact_secretlike};

#[test]
fn redacts_a_simple_address() {
    assert_eq!(redact_email("ada@example.org"), "a***@e***.org");
}

#[test]
fn no_local_part_or_first_label_survives() {
    let out = redact_email("ada@example.org");
    assert!(!out.contains("ada"), "local part leaked in {out:?}");
    assert!(!out.contains("example"), "domain label leaked in {out:?}");
    assert!(!out.contains("ada@example.org"));
}

#[test]
fn redacts_multi_label_domain_to_first_char_and_tld() {
    let out = redact_email("first.last@mail.example.co.uk");
    assert_eq!(out, "f***@m***.uk");
    for leaked in ["first", "last", "mail", "example"] {
        assert!(!out.contains(leaked), "{leaked} leaked in {out:?}");
    }
}

#[test]
fn redacts_dotless_domain() {
    let out = redact_email("bob@localhost");
    assert_eq!(out, "b***@l***");
    assert!(!out.contains("bob") && !out.contains("localhost"));
}

#[test]
fn non_address_input_is_fully_masked() {
    assert_eq!(redact_email("not-an-address"), "***");
    assert_eq!(redact_email(""), "***");
    assert_eq!(redact_email("@example.org"), "***");
}

#[test]
fn masks_hex_token() {
    let token = "a3f5c9d1b7e2049688fbca31de57902f";
    let out = redact_secretlike(token);
    assert_eq!(out, "***");
    assert!(!out.contains(token));
}

#[test]
fn masks_all_letter_hex_run() {
    // 32 hex chars, letters only (no digits): exercises the hex branch.
    let token = "deadbeefdeadbeefdeadbeefdeadbeef";
    assert_eq!(redact_secretlike(token), "***");
}

#[test]
fn masks_token_but_keeps_label_and_surroundings() {
    let token = "AKIAIOSFODNN7EXAMPLEabc123def456";
    let line = format!("using token={token} for request");
    let out = redact_secretlike(&line);
    assert!(out.contains("token="), "label dropped: {out:?}");
    assert!(out.contains("for request"), "suffix dropped: {out:?}");
    assert!(out.contains("***"), "token not masked: {out:?}");
    assert!(!out.contains(token), "token survived: {out:?}");
}

#[test]
fn masks_base64url_token_inside_text() {
    let token = "aGVsbG8td29ybGQtdGhpcy1pcy1hLXRva2Vu-9x_Z0";
    let out = redact_secretlike(&format!("[{token}]"));
    assert_eq!(out, "[***]");
    assert!(!out.contains(token));
}

#[test]
fn leaves_prose_and_short_identifiers_untouched() {
    for input in [
        "The quick brown fox jumps over the lazy dog.",
        "id=abc123",
        "deadbeef",
        "supercalifragilisticexpialidocious",
        "error: connection reset by peer",
    ] {
        assert_eq!(redact_secretlike(input), input, "prose altered: {input:?}");
    }
}
