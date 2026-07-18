//! Conservative redaction helpers for logs and diagnostics.
//!
//! These functions keep personal and secret data out of the log stream.
//! They are deliberately lossy: the goal is that no full email address and
//! no token-shaped string survives, even at the cost of masking the
//! occasional harmless value.

/// Replacement used when nothing may be revealed.
const MASK: &str = "***";

/// Shortest run of token characters that may be treated as secret-like.
const MIN_TOKEN_LEN: usize = 20;

/// Shortest all-hexadecimal run treated as secret-like on its own.
const MIN_HEX_LEN: usize = 32;

/// Redacts an email address for logging.
///
/// `ada@example.org` becomes `a***@e***.org`: the first character of the
/// local part and of the domain's first label are kept, the top-level
/// domain is kept, and everything else is masked. Input that is not shaped
/// like an address is replaced wholesale with `***`.
#[must_use]
pub fn redact_email(input: &str) -> String {
    match input.rsplit_once('@') {
        Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
            format!("{}@{}", masked_label(local), redact_domain(domain))
        }
        _ => MASK.to_owned(),
    }
}

/// Masks a label to its first character followed by `***`.
fn masked_label(label: &str) -> String {
    match label.chars().next() {
        Some(first) => format!("{first}{MASK}"),
        None => MASK.to_owned(),
    }
}

/// Masks a domain, preserving only its first character and the TLD.
fn redact_domain(domain: &str) -> String {
    match domain.rsplit_once('.') {
        Some((rest, tld)) if !rest.is_empty() && !tld.is_empty() => {
            format!("{}.{tld}", masked_label(rest))
        }
        _ => masked_label(domain),
    }
}

/// Masks base64/hex-looking tokens inside an arbitrary string.
///
/// Maximal runs of token characters (`A-Z`, `a-z`, `0-9`, `+`, `/`, `-`,
/// `_`) at least [`MIN_TOKEN_LEN`] long that look high-entropy — mixed
/// letters and digits, a base64 special character, or a long run of pure
/// hexadecimal — are replaced with `***`. Ordinary prose, short
/// identifiers, and punctuation pass through unchanged, so the helper is
/// safe to apply to whole log lines.
#[must_use]
pub fn redact_secretlike(input: &str) -> String {
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

/// Characters that may appear inside a base64/base64url/hex token.
///
/// The base64 padding character `=` is deliberately excluded so that
/// `key=value` log fragments keep their label instead of merging into the
/// masked value.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '-' | '_')
}

/// Appends `run` to `out`, masked if it looks secret-like, then clears it.
fn flush_run(run: &mut String, out: &mut String) {
    if run.is_empty() {
        return;
    }
    if looks_secretlike(run) {
        out.push_str(MASK);
    } else {
        out.push_str(run);
    }
    run.clear();
}

/// Heuristic deciding whether a token run should be masked.
fn looks_secretlike(run: &str) -> bool {
    let len = run.chars().count();
    if len < MIN_TOKEN_LEN {
        return false;
    }
    let has_digit = run.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = run.bytes().any(|b| b.is_ascii_alphabetic());
    let has_base64_special = run.bytes().any(|b| matches!(b, b'+' | b'/'));
    let all_hex = run.bytes().all(|b| b.is_ascii_hexdigit());
    (has_digit && has_alpha) || has_base64_special || (all_hex && len >= MIN_HEX_LEN)
}
