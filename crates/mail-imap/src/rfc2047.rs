//! Minimal RFC 2047 "encoded-word" decoding for header text.
//!
//! IMAP `ENVELOPE` fields arrive as raw bytes that may contain
//! `=?charset?B/Q?text?=` encoded words. This module decodes the common
//! cases (UTF-8, ISO-8859-1, Windows-1252, US-ASCII; Base64 and
//! Quoted-Printable) and passes everything else through as UTF-8 (lossy).
//! Undecodable input degrades gracefully rather than failing.

/// One parsed segment of a header value.
enum Segment {
    /// A decoded encoded-word.
    Encoded(String),
    /// A literal run of bytes, decoded as UTF-8 (lossy).
    Literal(String),
}

/// Decodes a header value that may contain RFC 2047 encoded words.
#[must_use]
pub(crate) fn decode(input: &[u8]) -> String {
    let segments = parse_segments(input);
    let mut out = String::new();
    for (idx, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Encoded(text) => out.push_str(text),
            Segment::Literal(text) => {
                if !drop_between_encoded(&segments, idx, text) {
                    out.push_str(text);
                }
            }
        }
    }
    out
}

/// True if `text` is whitespace separating two encoded words, which RFC
/// 2047 section 6.2 says to discard when displaying.
fn drop_between_encoded(segments: &[Segment], idx: usize, text: &str) -> bool {
    let ws_only = !text.is_empty() && text.bytes().all(|b| b.is_ascii_whitespace());
    let prev_encoded = idx
        .checked_sub(1)
        .and_then(|i| segments.get(i))
        .is_some_and(|s| matches!(s, Segment::Encoded(_)));
    let next_encoded = segments
        .get(idx + 1)
        .is_some_and(|s| matches!(s, Segment::Encoded(_)));
    ws_only && prev_encoded && next_encoded
}

/// Splits `input` into encoded-word and literal segments.
fn parse_segments(input: &[u8]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut literal: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if let Some((decoded, next)) = parse_encoded_word(input, i) {
            if !literal.is_empty() {
                segments.push(Segment::Literal(lossy(&literal)));
                literal.clear();
            }
            segments.push(Segment::Encoded(decoded));
            i = next;
        } else {
            literal.push(input[i]);
            i += 1;
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(lossy(&literal)));
    }
    segments
}

/// Attempts to parse a single `=?charset?enc?text?=` starting at `start`.
///
/// Returns the decoded text and the index just past the closing `?=`, or
/// `None` if the bytes there are not a well-formed encoded word.
fn parse_encoded_word(input: &[u8], start: usize) -> Option<(String, usize)> {
    let rest = &input[start..];
    if !rest.starts_with(b"=?") {
        return None;
    }
    let mut idx = 2;

    let charset_start = idx;
    while idx < rest.len() && rest[idx] != b'?' {
        if rest[idx] <= b' ' {
            return None;
        }
        idx += 1;
    }
    if idx >= rest.len() {
        return None;
    }
    let charset = &rest[charset_start..idx];
    idx += 1;

    let encoding = *rest.get(idx)?;
    idx += 1;
    if rest.get(idx) != Some(&b'?') {
        return None;
    }
    idx += 1;

    let text_start = idx;
    let mut found = false;
    while idx < rest.len() {
        if rest[idx] == b'?' && rest.get(idx + 1) == Some(&b'=') {
            found = true;
            break;
        }
        if rest[idx] <= b' ' {
            return None;
        }
        idx += 1;
    }
    if !found {
        return None;
    }
    let text = &rest[text_start..idx];

    let bytes = match encoding {
        b'B' | b'b' => base64_decode(text)?,
        b'Q' | b'q' => q_decode(text),
        _ => return None,
    };
    Some((decode_charset(charset, &bytes), start + idx + 2))
}

/// Decodes bytes according to `charset`, tolerating unknown charsets by
/// treating the bytes as UTF-8.
fn decode_charset(charset: &[u8], bytes: &[u8]) -> String {
    let name = charset.split(|&b| b == b'*').next().unwrap_or(charset);
    let upper = name.to_ascii_uppercase();
    match upper.as_slice() {
        b"ISO-8859-1" | b"ISO8859-1" | b"LATIN1" | b"L1" => {
            bytes.iter().map(|&b| b as char).collect()
        }
        b"WINDOWS-1252" | b"CP1252" => bytes.iter().map(|&b| windows_1252(b)).collect(),
        _ => lossy(bytes),
    }
}

/// Maps a Windows-1252 byte to its Unicode scalar value.
fn windows_1252(byte: u8) -> char {
    // Only 0x80..=0x9F differ from ISO-8859-1.
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        other => other as char,
    }
}

/// Decodes the RFC 2047 "Q" encoding (`_` is space, `=XX` is a hex byte).
fn q_decode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' => {
                if let (Some(hi), Some(lo)) = (
                    text.get(i + 1).and_then(|b| hex_value(*b)),
                    text.get(i + 2).and_then(|b| hex_value(*b)),
                ) {
                    out.push((hi << 4) | lo);
                    i += 3;
                } else {
                    out.push(b'=');
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Returns the numeric value of a hex digit, if it is one.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Decodes standard Base64, ignoring `=` padding, returning `None` on an
/// invalid symbol.
fn base64_decode(text: &[u8]) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::new();
    for &symbol in text {
        if symbol == b'=' {
            break;
        }
        let value = base64_value(symbol)?;
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Maps a standard Base64 alphabet symbol to its 6-bit value.
fn base64_value(symbol: u8) -> Option<u8> {
    match symbol {
        b'A'..=b'Z' => Some(symbol - b'A'),
        b'a'..=b'z' => Some(symbol - b'a' + 26),
        b'0'..=b'9' => Some(symbol - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// UTF-8 decoding that replaces invalid sequences instead of failing.
fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> String {
        decode(s.as_bytes())
    }

    #[test]
    fn plain_ascii_passes_through() {
        assert_eq!(dec("Hello world"), "Hello world");
    }

    #[test]
    fn decodes_base64_utf8() {
        // "=?UTF-8?B?w6RiYw==?=" is "äbc".
        assert_eq!(dec("=?UTF-8?B?w6RiYw==?="), "äbc");
    }

    #[test]
    fn decodes_quoted_printable() {
        assert_eq!(dec("=?UTF-8?Q?=C3=A4bc?="), "äbc");
        assert_eq!(dec("=?ISO-8859-1?Q?a_b?="), "a b");
    }

    #[test]
    fn decodes_iso_8859_1() {
        // 0xE4 is 'ä' in ISO-8859-1.
        assert_eq!(dec("=?ISO-8859-1?Q?=E4?="), "ä");
    }

    #[test]
    fn decodes_windows_1252_euro() {
        // 0x80 is the euro sign in Windows-1252.
        assert_eq!(dec("=?windows-1252?Q?=80?="), "€");
    }

    #[test]
    fn whitespace_between_encoded_words_is_dropped() {
        let input = "=?UTF-8?B?w6Q=?= =?UTF-8?B?w7Y=?=";
        assert_eq!(dec(input), "äö");
    }

    #[test]
    fn whitespace_around_literals_is_kept() {
        assert_eq!(dec("A =?UTF-8?B?w6Q=?= B"), "A ä B");
    }

    #[test]
    fn malformed_encoded_word_is_literal() {
        assert_eq!(dec("=?UTF-8?B?not closed"), "=?UTF-8?B?not closed");
        assert_eq!(dec("=?"), "=?");
    }

    #[test]
    fn mixed_literal_and_encoded() {
        assert_eq!(dec("Re: =?UTF-8?Q?Caf=C3=A9?= today"), "Re: Café today");
    }
}
