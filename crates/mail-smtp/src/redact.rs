//! Local redaction for SMTP reply lines.
//!
//! Server replies to `MAIL FROM`, `RCPT TO`, and `DATA` frequently echo the
//! envelope addresses (e.g. `250 2.1.5 Recipient <ada@example.org> OK`). Before
//! any reply text reaches a [`axomail_core::SendResult`] or a
//! [`axomail_core::MailError`] message it passes through here so that no full
//! address — and no obviously secret-shaped token — survives into diagnostics.
//!
//! This mirrors the intent of `axomail-security`'s redaction helpers without
//! pulling that crate (and its keychain/HTML dependencies) into the SMTP
//! adapter's tree.

/// Replacement for a value that must not be revealed.
const MASK: &str = "***";

/// Shortest run of token characters treated as possibly secret.
const MIN_TOKEN_LEN: usize = 20;

/// Formats `code` and a redacted `message` into a single reply line.
#[must_use]
pub(crate) fn redact_reply_line(code: u16, message: &str) -> String {
    let redacted = redact(message);
    if redacted.is_empty() {
        code.to_string()
    } else {
        format!("{code} {redacted}")
    }
}

/// Masks email addresses and secret-shaped tokens inside an arbitrary string.
///
/// Runs of address/token characters are inspected as units: anything shaped
/// like an email address is reduced to `f***@***` (keeping only the first
/// character of the local part for diagnostics), and any other run at least
/// [`MIN_TOKEN_LEN`] long that mixes letters and digits is replaced wholesale.
/// Ordinary words, status codes, and punctuation pass through unchanged.
#[must_use]
pub(crate) fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut run = String::new();
    for c in input.chars() {
        if is_token_char(c) {
            run.push(c);
        } else {
            flush_run(&mut run, &mut out);
            out.push(c);
        }
    }
    flush_run(&mut run, &mut out);
    out
}

/// Characters that can appear inside an address or an opaque token.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+' | '%' | '/')
}

/// Appends `run` to `out`, masked if it looks like an address or a secret.
fn flush_run(run: &mut String, out: &mut String) {
    if run.is_empty() {
        return;
    }
    if let Some(masked) = mask_email(run) {
        out.push_str(&masked);
    } else if looks_secretlike(run) {
        out.push_str(MASK);
    } else {
        out.push_str(run);
    }
    run.clear();
}

/// Returns a masked address if `run` is shaped like exactly one email address.
fn mask_email(run: &str) -> Option<String> {
    let (local, domain) = run.split_once('@')?;
    if local.is_empty() || domain.contains('@') || !domain.contains('.') {
        return None;
    }
    // Guard against `.` runs such as an enhanced status code that reached here.
    if domain.starts_with('.') || domain.ends_with('.') {
        return None;
    }
    let first = local.chars().next().unwrap_or('x');
    Some(format!("{first}{MASK}@{MASK}"))
}

/// Heuristic for a high-entropy token that should be masked on its own.
fn looks_secretlike(run: &str) -> bool {
    if run.chars().count() < MIN_TOKEN_LEN {
        return false;
    }
    let has_digit = run.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = run.bytes().any(|b| b.is_ascii_alphabetic());
    has_digit && has_alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_bare_and_bracketed_addresses() {
        assert_eq!(
            redact("Recipient ada@example.org OK"),
            "Recipient a***@*** OK"
        );
        assert_eq!(
            redact("2.1.5 <ada@example.org> accepted"),
            "2.1.5 <a***@***> accepted"
        );
    }

    #[test]
    fn keeps_status_codes_and_words() {
        assert_eq!(redact("2.0.0 OK"), "2.0.0 OK");
        assert_eq!(
            redact("Message accepted for delivery"),
            "Message accepted for delivery"
        );
    }

    #[test]
    fn masks_long_mixed_tokens() {
        let secret = "AbCd1234EfGh5678IjKl9012";
        assert_eq!(redact(secret), MASK);
    }

    #[test]
    fn reply_line_prefixes_code() {
        assert_eq!(redact_reply_line(250, "2.0.0 OK"), "250 2.0.0 OK");
        assert_eq!(redact_reply_line(250, ""), "250");
    }
}
