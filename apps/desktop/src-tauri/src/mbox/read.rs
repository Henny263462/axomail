//! mbox parsing (reader side).
//!
//! [`MboxReader`] pulls one message at a time from a buffered byte source so a
//! large archive is split and ingested incrementally, never fully resident in
//! memory. It parses the standard **mboxrd** container our writer produces
//! (and that Thunderbird / mutt / Apple Mail produce): `From ` postmarks
//! delimit messages and body lines matching `^>+From ` are un-escaped by one
//! `>`.
//!
//! It additionally honours a `Content-Length` header (the **mboxcl** variant
//! written by some older clients): when present it reads exactly that many body
//! bytes verbatim — but only after *verifying* the byte-exact split lands on a
//! real postmark boundary. A `Content-Length` that does not (a stray header in
//! an escaped mboxrd file) is ignored and the message is re-parsed by the
//! mboxrd rules, so a misleading header can never corrupt an import.

use std::io::{self, BufRead};

use axomail_core::MailError;

/// Streaming splitter over a buffered reader of mbox bytes.
pub struct MboxReader<R: BufRead> {
    reader: R,
    /// One-line pushback: a postmark peeked while ending one message is held
    /// here for the next [`MboxReader::next_message`] call to consume.
    pending: Option<Vec<u8>>,
}

impl<R: BufRead> MboxReader<R> {
    /// Wraps `reader`. The caller is responsible for confirming the source is
    /// actually mbox (starts with a `From ` postmark); a source that never
    /// contains one yields no messages.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            pending: None,
        }
    }

    /// Returns the next message's raw RFC 5322 bytes, or `None` at end of
    /// input. Leading non-postmark lines before the first message are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Filesystem`] on an underlying read error.
    pub fn next_message(&mut self) -> Result<Option<Vec<u8>>, MailError> {
        if !self.seek_postmark()? {
            return Ok(None);
        }
        let (header, content_length) = self.read_header_block()?;
        match content_length {
            Some(n) => self.read_with_content_length(header, n),
            None => Ok(Some(self.read_mboxrd_body(header))),
        }
    }

    /// Advances past the postmark line that opens the next message (discarding
    /// any leading blank/junk lines). Returns whether a postmark was found
    /// before EOF.
    fn seek_postmark(&mut self) -> Result<bool, MailError> {
        while let Some(line) = self.read_line()? {
            if is_postmark(&line) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Reads the header block (up to and including the blank separator line),
    /// returning the raw lines and any `Content-Length` value found.
    fn read_header_block(&mut self) -> Result<(Vec<Vec<u8>>, Option<usize>), MailError> {
        let mut header = Vec::new();
        let mut content_length = None;
        while let Some(line) = self.read_line()? {
            let blank = is_blank(&line);
            if content_length.is_none() {
                content_length = parse_content_length(&line);
            }
            header.push(line);
            if blank {
                break;
            }
        }
        Ok((header, content_length))
    }

    /// Applies a `Content-Length` of `n`, but only if the byte-exact body ends
    /// on a genuine postmark boundary (EOF, or blank separators then a
    /// postmark); otherwise the header value is stray and the message is
    /// re-parsed by the mboxrd rules with everything un-escaped.
    fn read_with_content_length(
        &mut self,
        header: Vec<Vec<u8>>,
        n: usize,
    ) -> Result<Option<Vec<u8>>, MailError> {
        let body = self.read_exact_upto(n)?;
        match self.peek_to_boundary()? {
            Boundary::Clean(postmark) => {
                // The verbatim body landed on blank separators then a postmark /
                // EOF: trust the Content-Length (genuine mboxcl). The separators
                // were consumed by the peek and are dropped.
                let mut msg = concat_lines(&header);
                msg.extend_from_slice(&body);
                self.pending = postmark;
                Ok(Some(msg))
            }
            Boundary::Stray(peeked) => {
                // Stray Content-Length in an escaped mboxrd file: re-parse. The
                // byte-exact read may have cut a line, so reassemble every byte
                // consumed since the postmark verbatim and re-split into lines
                // before un-escaping — piecewise un-escaping across a cut is
                // wrong. Scanning then continues to the next postmark.
                let mut raw = concat_lines(&header);
                raw.extend_from_slice(&body);
                for line in &peeked {
                    raw.extend_from_slice(line);
                }
                let mut msg = Vec::new();
                append_unescaped_bytes(&mut msg, &raw);
                self.scan_body_lines(&mut msg)?;
                trim_separator(&mut msg);
                Ok(Some(msg))
            }
        }
    }

    /// Reads the body of an mboxrd message: un-escaped lines up to the next
    /// postmark (pushed back) or EOF, with the trailing blank separator trimmed.
    fn read_mboxrd_body(&mut self, header: Vec<Vec<u8>>) -> Vec<u8> {
        let mut msg = concat_unescaped(&header);
        // A read error here is surfaced as a truncated message rather than
        // aborting; callers stream message-by-message and a disk error is rare.
        let _ = self.scan_body_lines(&mut msg);
        trim_separator(&mut msg);
        msg
    }

    /// Appends un-escaped body lines to `msg` until the next postmark (pushed
    /// back into `pending`) or EOF.
    fn scan_body_lines(&mut self, msg: &mut Vec<u8>) -> Result<(), MailError> {
        while let Some(line) = self.read_line()? {
            if is_postmark(&line) {
                self.pending = Some(line);
                return Ok(());
            }
            msg.extend_from_slice(&unescape(&line));
        }
        Ok(())
    }

    /// Reads forward from the end of a byte-exact body to classify the split.
    /// A run of only blank separators ending at a postmark or EOF is
    /// [`Boundary::Clean`] (genuine mboxcl); the first non-blank, non-postmark
    /// line means the split fell inside real content — [`Boundary::Stray`],
    /// carrying every line consumed (blank separators included, since in an
    /// mboxrd file they are body content).
    fn peek_to_boundary(&mut self) -> Result<Boundary, MailError> {
        let mut consumed = Vec::new();
        while let Some(line) = self.read_line()? {
            if is_postmark(&line) {
                return Ok(Boundary::Clean(Some(line)));
            }
            let blank = is_blank(&line);
            consumed.push(line);
            if !blank {
                return Ok(Boundary::Stray(consumed));
            }
        }
        Ok(Boundary::Clean(None))
    }

    /// Reads up to `n` bytes (fewer only at EOF). `pending` is always empty at
    /// this point (it holds only postmarks, set at message boundaries).
    fn read_exact_upto(&mut self, n: usize) -> Result<Vec<u8>, MailError> {
        let mut buf = vec![0u8; n];
        let mut filled = 0;
        while filled < n {
            let got = self.reader.read(&mut buf[filled..]).map_err(io_error)?;
            if got == 0 {
                buf.truncate(filled);
                break;
            }
            filled += got;
        }
        Ok(buf)
    }

    /// Reads one line including its trailing `\n` (the final line may lack
    /// one), or `None` at EOF. Honours the one-line pushback first.
    fn read_line(&mut self) -> Result<Option<Vec<u8>>, MailError> {
        if let Some(line) = self.pending.take() {
            return Ok(Some(line));
        }
        let mut buf = Vec::new();
        let read = self.reader.read_until(b'\n', &mut buf).map_err(io_error)?;
        if read == 0 { Ok(None) } else { Ok(Some(buf)) }
    }
}

/// Outcome of peeking past a byte-exact (`Content-Length`) body to the next
/// message boundary.
enum Boundary {
    /// Only blank separators (dropped) preceded the boundary: the body length
    /// was honoured. Carries the postmark line that starts the next message, or
    /// `None` at EOF.
    Clean(Option<Vec<u8>>),
    /// A non-blank line appeared before any postmark: the `Content-Length` was
    /// stray. Carries every line consumed by the peek, all real message content.
    Stray(Vec<Vec<u8>>),
}

/// Whether `line` opens a message: it begins with the `From ` postmark marker.
fn is_postmark(line: &[u8]) -> bool {
    line.starts_with(b"From ")
}

/// Whether `line` is the empty header/body separator (`\n`, `\r\n`, or an
/// empty final line).
fn is_blank(line: &[u8]) -> bool {
    line.is_empty() || line == b"\n" || line == b"\r\n"
}

/// Removes one leading `>` from a line matching `^>+From ` (the inverse of the
/// writer's escaping); every other line is returned unchanged.
fn unescape(line: &[u8]) -> Vec<u8> {
    if line.first() == Some(&b'>') {
        let mut i = 0;
        while i < line.len() && line[i] == b'>' {
            i += 1;
        }
        if line[i..].starts_with(b"From ") {
            return line[1..].to_vec();
        }
    }
    line.to_vec()
}

/// Concatenates raw lines verbatim.
fn concat_lines(lines: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        out.extend_from_slice(line);
    }
    out
}

/// Concatenates lines with mboxrd un-escaping applied.
fn concat_unescaped(lines: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        out.extend_from_slice(&unescape(line));
    }
    out
}

/// Splits `bytes` into lines and appends each un-escaped to `msg`.
fn append_unescaped_bytes(msg: &mut Vec<u8>, bytes: &[u8]) {
    let mut start = 0;
    while start < bytes.len() {
        let end = match bytes[start..].iter().position(|&b| b == b'\n') {
            Some(offset) => start + offset + 1,
            None => bytes.len(),
        };
        msg.extend_from_slice(&unescape(&bytes[start..end]));
        start = end;
    }
}

/// Drops the single blank-line separator the writer places before the next
/// postmark, restoring the message's own terminating newline.
fn trim_separator(msg: &mut Vec<u8>) {
    if msg.ends_with(b"\n\n") {
        msg.pop();
    }
}

/// Parses a `Content-Length: N` header line, or `None` for any other line.
fn parse_content_length(line: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(line).ok()?;
    let (name, value) = text.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case("Content-Length") {
        return None;
    }
    value.trim().parse::<usize>().ok()
}

/// Maps an IO error to a redacted [`MailError::Filesystem`] (kind only, never
/// a path or OS message).
fn io_error(error: io::Error) -> MailError {
    MailError::Filesystem {
        message: format!("reading the mbox file: {}", error.kind()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::io::Cursor;

    use super::*;

    fn read_all(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut reader = MboxReader::new(Cursor::new(bytes.to_vec()));
        let mut out = Vec::new();
        while let Some(msg) = reader.next_message().unwrap() {
            out.push(msg);
        }
        out
    }

    #[test]
    fn empty_input_yields_no_messages() {
        assert!(read_all(b"").is_empty());
    }

    #[test]
    fn splits_two_messages_on_postmarks() {
        let mbox = b"From a@b Thu Jan 01 00:00:00 1970\r\nSubject: one\r\n\r\nbody one\r\n\nFrom c@d Thu Jan 01 00:00:00 1970\r\nSubject: two\r\n\r\nbody two\r\n\n";
        let msgs = read_all(mbox);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], b"Subject: one\r\n\r\nbody one\r\n");
        assert_eq!(msgs[1], b"Subject: two\r\n\r\nbody two\r\n");
    }

    #[test]
    fn unescapes_from_lines() {
        let mbox = b"From a@b Thu Jan 01 00:00:00 1970\r\nSubject: x\r\n\r\n>From the desk\r\n>>From already\r\n\n";
        let msgs = read_all(mbox);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0],
            b"Subject: x\r\n\r\nFrom the desk\r\n>From already\r\n"
        );
    }

    #[test]
    fn honours_a_genuine_content_length_with_an_unescaped_body() {
        // mboxcl: the body is 14 bytes and NOT escaped; a bare "From " line in
        // the body is content, not a separator, and must survive intact.
        let body = "From here\r\nend\r\n"; // 16 bytes
        let mbox = format!(
            "From a@b Thu Jan 01 00:00:00 1970\r\nContent-Length: {}\r\n\r\n{}\nFrom c@d Thu Jan 01 00:00:00 1970\r\nSubject: two\r\n\r\ntwo\r\n\n",
            body.len(),
            body,
        );
        let msgs = read_all(mbox.as_bytes());
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], b"Content-Length: 16\r\n\r\nFrom here\r\nend\r\n");
        assert_eq!(msgs[1], b"Subject: two\r\n\r\ntwo\r\n");
    }

    #[test]
    fn ignores_a_stray_content_length_in_an_escaped_file() {
        // The header claims 5 bytes but the file is really mboxrd (the body is
        // escaped and longer). The reader must fall back to postmark splitting
        // and un-escape, not truncate at 5 bytes.
        let mbox = b"From a@b Thu Jan 01 00:00:00 1970\r\nContent-Length: 5\r\n\r\n>From escaped line\r\nmore body\r\n\n";
        let msgs = read_all(mbox);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0],
            b"Content-Length: 5\r\n\r\nFrom escaped line\r\nmore body\r\n"
        );
    }

    #[test]
    fn skips_leading_blank_lines_before_the_first_message() {
        let mbox = b"\n\nFrom a@b Thu Jan 01 00:00:00 1970\r\nS: x\r\n\r\nbody\r\n\n";
        let msgs = read_all(mbox);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], b"S: x\r\n\r\nbody\r\n");
    }
}
