//! Low-level MIME byte plumbing for the RFC 3156 layer: CRLF canonicalisation,
//! header/body splitting, logical-header parsing, multipart boundary handling,
//! armored-block extraction, and boundary generation.
//!
//! These operate on raw bytes and never mutate content, so a signed part can be
//! recovered byte-for-byte (the load-bearing invariant for signature
//! verification). They live apart from the PGP/MIME orchestration in
//! [`super::mime`] to keep each file within the size gate.

/// Normalises lone `\n` / `\r` to CRLF.
pub(super) fn canonicalize_crlf(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'\r' if input.get(i + 1) == Some(&b'\n') => {
                out.extend_from_slice(b"\r\n");
                i += 2;
            }
            b'\r' | b'\n' => {
                out.extend_from_slice(b"\r\n");
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

/// Splits a message into its header block (without the terminating blank line)
/// and its body, at the first CRLF CRLF (or LF LF) boundary.
pub(super) fn split_headers_body(raw: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if let Some(pos) = find(raw, b"\r\n\r\n", 0) {
        (raw[..pos].to_vec(), raw[pos + 4..].to_vec())
    } else if let Some(pos) = find(raw, b"\n\n", 0) {
        (raw[..pos].to_vec(), raw[pos + 2..].to_vec())
    } else {
        (raw.to_vec(), Vec::new())
    }
}

/// Splits a header block into logical headers, rejoining folded continuation
/// lines (those beginning with a space or tab) onto their header.
pub(super) fn parse_logical_headers(block: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(block);
    let mut headers: Vec<String> = Vec::new();
    for line in text.split("\r\n").flat_map(|l| l.split('\n')) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with([' ', '\t'])
            && let Some(last) = headers.last_mut()
        {
            last.push_str("\r\n");
            last.push_str(line);
        } else {
            headers.push(line.to_owned());
        }
    }
    headers
}

/// The header name (text before the first `:`), trimmed.
pub(super) fn header_name(line: &str) -> &str {
    line.split_once(':').map_or("", |(name, _)| name.trim())
}

/// Whether a logical header is a `Content-*` header (moves into the inner
/// entity); everything else stays as an envelope header.
pub(super) fn is_content_header(line: &str) -> bool {
    header_name(line)
        .to_ascii_lowercase()
        .starts_with("content-")
}

/// Reads a header's unfolded value from a header block by case-insensitive name.
pub(super) fn header_value(block: &[u8], name: &str) -> Option<String> {
    parse_logical_headers(block).into_iter().find_map(|line| {
        let (header, value) = line.split_once(':')?;
        header
            .trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.replace(['\r', '\n'], " ").trim().to_owned())
    })
}

/// Extracts the `boundary` parameter from a Content-Type value.
pub(super) fn boundary_of(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let start = lower.find("boundary=")? + "boundary=".len();
    let rest = content_type[start..].trim_start();
    let value = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next().unwrap_or("")
    } else {
        rest.split([';', ' ', '\t']).next().unwrap_or("")
    };
    (!value.is_empty()).then(|| value.to_owned())
}

/// Splits a multipart body into the exact byte range of each part's content,
/// between consecutive `--boundary` delimiter lines.
pub(super) fn multipart_parts<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let dash = format!("--{boundary}");
    let needle = dash.as_bytes();
    let mut delims: Vec<usize> = Vec::new();
    let mut i = 0;
    while let Some(pos) = find(body, needle, i) {
        let at_line_start =
            pos == 0 || body[pos - 1] == b'\n' || (pos >= 2 && &body[pos - 2..pos] == b"\r\n");
        if at_line_start {
            delims.push(pos);
        }
        i = pos + needle.len();
    }
    let mut parts = Vec::new();
    for window in delims.windows(2) {
        let (d0, d1) = (window[0], window[1]);
        let token_end = d0 + needle.len();
        let content_start = if body[token_end..].starts_with(b"\r\n") {
            token_end + 2
        } else if body.get(token_end) == Some(&b'\n') {
            token_end + 1
        } else {
            token_end
        };
        let content_end = if d1 >= 2 && &body[d1 - 2..d1] == b"\r\n" {
            d1 - 2
        } else if d1 >= 1 && body[d1 - 1] == b'\n' {
            d1 - 1
        } else {
            d1
        };
        if content_start <= content_end {
            parts.push(&body[content_start..content_end]);
        }
    }
    parts
}

/// Extracts the first ASCII-armored OpenPGP block of the given kind (e.g.
/// `MESSAGE`, `SIGNATURE`) from `data`, as a UTF-8 string.
pub(super) fn extract_armor(data: &[u8], kind: &str) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let begin = format!("-----BEGIN PGP {kind}-----");
    let end = format!("-----END PGP {kind}-----");
    let start = text.find(&begin)?;
    let end_at = text[start..].find(&end)? + start + end.len();
    Some(text[start..end_at].to_owned())
}

/// Normalises an armored block to CRLF line endings with no trailing newline, so
/// it slots cleanly into a MIME part.
pub(super) fn normalize_armor(armored: &str) -> String {
    let unified = armored.replace("\r\n", "\n");
    unified
        .trim_end_matches('\n')
        .split('\n')
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Generates a boundary unlikely to collide with content.
pub(super) fn make_boundary(tag: &str) -> String {
    format!(
        "=-=axomail-{tag}-{:016x}{:016x}=-=",
        rand::random::<u64>(),
        rand::random::<u64>()
    )
}

/// Finds the first index of `needle` in `haystack` at or after `from`.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|pos| pos + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_parsing_handles_quotes_and_params() {
        assert_eq!(
            boundary_of("multipart/signed; boundary=\"abc\"; micalg=\"pgp-sha256\""),
            Some("abc".to_owned())
        );
        assert_eq!(
            boundary_of("multipart/mixed; boundary=xyz"),
            Some("xyz".to_owned())
        );
    }

    #[test]
    fn content_headers_are_partitioned_from_envelope() {
        assert!(is_content_header("Content-Type: text/plain"));
        assert!(is_content_header("content-transfer-encoding: 7bit"));
        assert!(!is_content_header("From: a@b.c"));
        assert!(!is_content_header("MIME-Version: 1.0"));
    }

    #[test]
    fn multipart_parts_recovers_exact_bytes() {
        let body = b"--B\r\nContent-Type: text/plain\r\n\r\nhi\r\n--B\r\nsig\r\n--B--\r\n";
        let parts = multipart_parts(body, "B");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], b"Content-Type: text/plain\r\n\r\nhi");
        assert_eq!(parts[1], b"sig");
    }
}
