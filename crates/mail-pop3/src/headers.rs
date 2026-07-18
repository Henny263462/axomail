//! A minimal header scan for building preliminary POP3 envelopes, plus the
//! digest-identity fallback for servers without `UIDL`.
//!
//! The scan is deliberately small: it unfolds header lines, then reads
//! `Subject`, `From`, and `Date` well enough for a list view. It does **not**
//! decode RFC 2047 encoded words or handle full address-list grammar — that
//! is `axomail-parser`'s job once the whole message is downloaded. Envelope
//! fields here are best-effort hints, refined later by the real parser.

use axomail_core::{Address, EmailAddress, UtcTimestamp};

use crate::date;

/// Header fields extracted from a message's header block.
pub(crate) struct ScannedHeaders {
    /// Raw (still possibly RFC 2047-encoded) subject text.
    pub(crate) subject: Option<String>,
    /// Sender addresses (zero or one; the first address only).
    pub(crate) from: Vec<Address>,
    /// Parsed `Date:` header, if present and recognizable.
    pub(crate) date: Option<UtcTimestamp>,
}

/// Scans the header block of `raw` (as returned by `TOP number 0`).
///
/// Parsing stops at the first empty line, which ends the header section.
#[must_use]
pub(crate) fn scan(raw: &[u8]) -> ScannedHeaders {
    let text = String::from_utf8_lossy(raw);
    let mut subject = None;
    let mut from = Vec::new();
    let mut date = None;

    for (name, value) in unfold(&text) {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "subject" if subject.is_none() => subject = Some(value.trim().to_owned()),
            "from" if from.is_empty() => from = parse_from(&value),
            "date" if date.is_none() => date = date::parse(value.trim()),
            _ => {}
        }
    }

    ScannedHeaders {
        subject,
        from,
        date,
    }
}

/// A stable, content-derived identity for a message on a server that does
/// not support `UIDL`.
///
/// The identity is a 64-bit FNV-1a hash of the message's header bytes mixed
/// with its octet size, rendered as a prefixed hex string.
///
/// # Stability risks
///
/// Unlike a real `UIDL`, this identity is only as stable as the header
/// bytes the server returns. If the server rewrites headers between
/// sessions (adding or reordering `Received:` lines, for example) the
/// digest changes and the message looks new, causing a duplicate. Two
/// distinct messages with byte-identical headers *and* equal size would
/// collide and one could be skipped. It is therefore a last resort, used
/// only when neither `UIDL` nor a stabler identity is available; the
/// adapter refuses to sync a server that supports neither `UIDL` nor `TOP`.
#[must_use]
pub(crate) fn digest_identity(headers: &[u8], size: u64) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &byte in headers {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    for byte in size.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("d:{hash:016x}:{size}")
}

/// Unfolds a header block into `(name, value)` pairs, joining continuation
/// lines (those starting with a space or tab) onto the preceding field and
/// stopping at the first blank line.
fn unfold(text: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if let Some(last) = fields.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            fields.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    fields
}

/// Parses the first address of a `From:` value into a core [`Address`].
///
/// Only the first comma-separated entry is considered, and the parse is
/// intentionally light: a `Name <local@host>` form, or a bare address.
fn parse_from(value: &str) -> Vec<Address> {
    let first = value.split(',').next().unwrap_or(value).trim();
    if first.is_empty() {
        return Vec::new();
    }

    let (name, addr) = match (first.find('<'), first.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            (first[..open].trim(), first[open + 1..close].trim())
        }
        _ => ("", first),
    };

    match EmailAddress::parse(addr) {
        Ok(email) => {
            let display = name.trim().trim_matches('"').trim();
            vec![Address {
                display_name: (!display.is_empty()).then(|| display.to_owned()),
                email,
            }]
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = concat!(
        "Return-Path: <bounce@example.org>\r\n",
        "From: \"Alice Example\" <alice@example.org>\r\n",
        "Subject: Lunch\r\n",
        " on Thursday\r\n",
        "Date: Tue, 14 Jul 2026 09:15:00 +0000\r\n",
        "\r\n",
        "body should be ignored\r\n",
    )
    .as_bytes();

    #[test]
    fn scans_subject_from_and_date() {
        let scanned = scan(SAMPLE);
        assert_eq!(scanned.subject.as_deref(), Some("Lunch on Thursday"));
        assert_eq!(scanned.from.len(), 1);
        assert_eq!(
            scanned.from[0].display_name.as_deref(),
            Some("Alice Example")
        );
        assert_eq!(scanned.from[0].email.as_str(), "alice@example.org");
        assert!(scanned.date.is_some());
    }

    #[test]
    fn bare_address_without_name() {
        let from = parse_from("bob@example.com");
        assert_eq!(from.len(), 1);
        assert!(from[0].display_name.is_none());
        assert_eq!(from[0].email.as_str(), "bob@example.com");
    }

    #[test]
    fn unparseable_from_is_dropped() {
        assert!(parse_from("not an address").is_empty());
        assert!(parse_from("").is_empty());
    }

    #[test]
    fn digest_is_stable_and_size_sensitive() {
        let a = digest_identity(b"From: a@b.co\r\n", 100);
        let b = digest_identity(b"From: a@b.co\r\n", 100);
        let c = digest_identity(b"From: a@b.co\r\n", 101);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("d:"));
    }
}
