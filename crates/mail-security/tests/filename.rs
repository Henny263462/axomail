//! Table-driven tests for attachment filename sanitization.
#![allow(clippy::unwrap_used)]

use axomail_security::sanitize_attachment_filename;

/// `(input, expected)` pairs with fully determined outputs.
const CASES: &[(&str, &str)] = &[
    // Path traversal and separators.
    ("../../etc/passwd", ".._.._etc_passwd"),
    ("..\\..\\windows\\system32", ".._.._windows_system32"),
    ("/etc/passwd", "_etc_passwd"),
    ("foo/bar/baz.txt", "foo_bar_baz.txt"),
    ("..", "attachment"),
    (".", "attachment"),
    ("...", "attachment"),
    // Windows-reserved device names, with and without extension.
    ("CON", "_CON"),
    ("con.txt", "_con.txt"),
    ("NUL", "_NUL"),
    ("PRN.pdf", "_PRN.pdf"),
    ("AUX", "_AUX"),
    ("COM1", "_COM1"),
    ("LPT9", "_LPT9"),
    ("CON ", "_CON"),
    ("NUL.", "_NUL"),
    // Names that only look reserved.
    ("COM0", "COM0"),
    ("COM10", "COM10"),
    ("CONSOLE.txt", "CONSOLE.txt"),
    ("LPT.txt", "LPT.txt"),
    // Trailing dots and spaces (Windows would strip these).
    ("report.pdf.", "report.pdf"),
    ("report.pdf   ", "report.pdf"),
    ("  spaced.txt  ", "spaced.txt"),
    // Forbidden and control characters.
    ("a<b>c:d|e?f*g.txt", "a_b_c_d_e_f_g.txt"),
    ("quote\"name.txt", "quote_name.txt"),
    ("a\u{0}b.txt", "ab.txt"),
    ("a\tb\nc.txt", "abc.txt"),
    // Unicode is preserved.
    ("café.pdf", "café.pdf"),
    ("résumé.docx", "résumé.docx"),
    ("文件.txt", "文件.txt"),
    ("photo📷.png", "photo📷.png"),
    (".gitignore", ".gitignore"),
    // Empty and whitespace-only inputs.
    ("", "attachment"),
    ("   ", "attachment"),
];

#[test]
fn produces_expected_outputs() {
    for (input, expected) in CASES {
        assert_eq!(
            sanitize_attachment_filename(input),
            *expected,
            "input {input:?}"
        );
    }
}

#[test]
fn every_output_is_a_safe_single_component() {
    let long = "x".repeat(500);
    let mut inputs: Vec<String> = CASES.iter().map(|(input, _)| (*input).to_owned()).collect();
    inputs.push(long.clone());
    inputs.push(format!("{long}.tar.gz"));
    inputs.push(format!("../{long}"));

    for input in inputs {
        let out = sanitize_attachment_filename(&input);
        assert!(!out.is_empty(), "empty output for {input:?}");
        assert!(
            !out.contains('/') && !out.contains('\\'),
            "separator survived in {out:?}"
        );
        assert!(out != "." && out != "..", "dot component for {input:?}");
        assert!(
            !out.chars().any(char::is_control),
            "control char survived in {out:?}"
        );
        assert!(
            out.chars().count() <= 150,
            "output too long ({}) for {input:?}",
            out.chars().count()
        );
    }
}

#[test]
fn long_name_truncation_keeps_extension_and_length() {
    let input = format!("{}.pdf", "a".repeat(400));
    let out = sanitize_attachment_filename(&input);
    assert!(out.ends_with(".pdf"));
    assert_eq!(out.chars().count(), 150);
}

#[test]
fn long_name_without_extension_is_truncated() {
    let out = sanitize_attachment_filename(&"z".repeat(400));
    assert_eq!(out, "z".repeat(150));
}
