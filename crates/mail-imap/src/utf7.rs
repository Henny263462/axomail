//! IMAP modified UTF-7 for mailbox names (RFC 3501 section 5.1.3).
//!
//! Printable ASCII represents itself except `&`, which is written `&-`.
//! Runs of other characters are encoded as UTF-16BE in a modified BASE64
//! alphabet (`/` replaced by `,`, no padding) delimited by `&` and `-`.
//!
//! Decoding is best-effort and never fails: malformed shift sequences are
//! replaced with U+FFFD so folder discovery is not derailed by one bad
//! name.

/// Modified BASE64 alphabet: standard, but with `,` in place of `/`.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";

/// Decodes a modified UTF-7 mailbox name into UTF-8.
#[must_use]
pub(crate) fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte != b'&' {
            out.push(byte as char);
            i += 1;
            continue;
        }
        // A shift sequence: find the terminating '-'.
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'-' {
            end += 1;
        }
        if end == start {
            // "&-" is a literal ampersand.
            out.push('&');
        } else {
            decode_shift(&bytes[start..end], &mut out);
        }
        // Skip the run and its terminating '-' (if present).
        i = if end < bytes.len() { end + 1 } else { end };
    }
    out
}

/// Decodes one modified-BASE64 run into `out`, appending U+FFFD on error.
fn decode_shift(run: &[u8], out: &mut String) {
    let Some(bytes) = base64_decode(run) else {
        out.push('\u{FFFD}');
        return;
    };
    if bytes.len() % 2 != 0 {
        out.push('\u{FFFD}');
        return;
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    for chunk in char::decode_utf16(units) {
        out.push(chunk.unwrap_or('\u{FFFD}'));
    }
}

/// Encodes a UTF-8 mailbox name into modified UTF-7.
#[must_use]
pub(crate) fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut run: Vec<u16> = Vec::new();

    for ch in input.chars() {
        if is_direct(ch) {
            flush_run(&mut run, &mut out);
            out.push(ch);
        } else if ch == '&' {
            flush_run(&mut run, &mut out);
            out.push_str("&-");
        } else {
            let mut buf = [0u16; 2];
            run.extend_from_slice(ch.encode_utf16(&mut buf));
        }
    }
    flush_run(&mut run, &mut out);
    out
}

/// Flushes a pending run of UTF-16 units as a `&...-` shift sequence.
fn flush_run(run: &mut Vec<u16>, out: &mut String) {
    if run.is_empty() {
        return;
    }
    let mut bytes = Vec::with_capacity(run.len() * 2);
    for unit in run.drain(..) {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    out.push('&');
    out.push_str(&base64_encode(&bytes));
    out.push('-');
}

/// True if `ch` represents itself directly (printable ASCII except `&`).
fn is_direct(ch: char) -> bool {
    matches!(ch, '\u{20}'..='\u{25}' | '\u{27}'..='\u{7e}')
}

/// Encodes bytes with the modified BASE64 alphabet, without padding.
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(n >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 0x3f] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 0x3f] as char);
        }
    }
    out
}

/// Decodes a modified-BASE64 run, returning `None` on any invalid symbol.
fn base64_decode(run: &[u8]) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::new();
    for &symbol in run {
        let value = ALPHABET.iter().position(|&c| c == symbol)? as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // Any leftover bits must be zero padding; reject stray set bits.
    if bits > 0 && (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_names_are_unchanged() {
        for name in ["INBOX", "Sent", "Projects/2026", "a.b.c"] {
            assert_eq!(decode(name), name);
            assert_eq!(encode(name), name);
        }
    }

    #[test]
    fn literal_ampersand_round_trips() {
        assert_eq!(decode("Mail &- News"), "Mail & News");
        assert_eq!(encode("Mail & News"), "Mail &- News");
    }

    #[test]
    fn decodes_known_examples() {
        // From RFC 3501: "&Jjo-" is a smiley; "Geschäfte".
        assert_eq!(decode("&Jjo-"), "\u{263a}");
        assert_eq!(decode("Gesch&AOQ-fte"), "Geschäfte");
        // RFC 2060 example: "~peter/mail/&U,BTFw-/&ZeVnLIqe-".
        assert_eq!(
            decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-"),
            "~peter/mail/\u{53f0}\u{5317}/\u{65e5}\u{672c}\u{8a9e}"
        );
    }

    #[test]
    fn encodes_non_ascii() {
        assert_eq!(encode("Geschäfte"), "Gesch&AOQ-fte");
        assert_eq!(encode("\u{263a}"), "&Jjo-");
    }

    #[test]
    fn round_trips_mixed_and_astral() {
        for name in [
            "Geschäfte",
            "\u{263a}",
            "台北/日本語",
            "Ünïçödë Folder",
            "emoji \u{1f600} folder",
        ] {
            assert_eq!(decode(&encode(name)), name, "round trip failed for {name}");
        }
    }

    #[test]
    fn malformed_shift_yields_replacement_not_panic() {
        // Unterminated / invalid base64 must not panic.
        let decoded = decode("&###-");
        assert!(decoded.contains('\u{FFFD}'));
        let _ = decode("&");
        let _ = decode("&AOQ");
    }
}
