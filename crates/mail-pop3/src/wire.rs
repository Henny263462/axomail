//! Low-level line framing for the POP3 wire protocol.
//!
//! POP3 is line-oriented: commands and single-line replies are terminated
//! by `CRLF`, and multi-line replies end with a line containing only `.`.
//! [`Wire`] owns the byte stream plus a small read buffer and exposes just
//! enough to read status lines, read multi-line bodies (with dot-stuffing
//! reversed), and write command lines. It is generic over the transport so
//! the same code serves the plaintext `STLS` handshake phase and the
//! encrypted session that follows.
//!
//! Reading is deliberately tolerant of line-ending laxness: a line may be
//! terminated by `CRLF` or a bare `LF`, and a final unterminated line at
//! end of stream is still returned.

use axomail_core::MailError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::map_io_error;

/// Upper bound on a single line, guarding against a peer that never sends a
/// terminator. Generous enough for any real header or body line.
const MAX_LINE_LEN: usize = 1 << 20;

/// Size of each read from the underlying stream when the buffer is drained.
const READ_CHUNK: usize = 8 * 1024;

/// Offset past which the consumed prefix of the buffer is compacted away.
const COMPACT_THRESHOLD: usize = 8 * 1024;

/// A parsed single-line status reply.
pub(crate) enum Reply {
    /// A `+OK` reply, carrying the (unparsed) text after the indicator.
    Ok(String),
    /// A `-ERR` reply; the server-supplied text is intentionally dropped.
    Err,
}

/// A buffered, line-oriented view over a POP3 byte stream.
pub(crate) struct Wire<S> {
    stream: S,
    buf: Vec<u8>,
    start: usize,
}

impl<S> Wire<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Wraps a freshly connected stream.
    pub(crate) fn new(stream: S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            start: 0,
        }
    }

    /// Splits the wire back into its stream and any bytes already buffered
    /// but not yet consumed.
    ///
    /// Used at the `STLS` boundary: the caller must verify no extra bytes
    /// were buffered before handing the raw stream to the TLS handshake.
    pub(crate) fn into_parts(self) -> (S, Vec<u8>) {
        (self.stream, self.buf[self.start..].to_vec())
    }

    /// Reads one line, returned without its trailing `CRLF`/`LF`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Network`] if the stream closes before any byte
    /// of a line arrives, and [`MailError::Protocol`] if a line exceeds
    /// [`MAX_LINE_LEN`] without a terminator.
    pub(crate) async fn read_line(&mut self) -> Result<Vec<u8>, MailError> {
        loop {
            if let Some(rel) = self.buf[self.start..].iter().position(|&b| b == b'\n') {
                let newline = self.start + rel;
                let mut end = newline;
                if end > self.start && self.buf[end - 1] == b'\r' {
                    end -= 1;
                }
                let line = self.buf[self.start..end].to_vec();
                self.start = newline + 1;
                self.compact();
                return Ok(line);
            }

            if self.buf.len() - self.start > MAX_LINE_LEN {
                return Err(MailError::Protocol {
                    message: "the server sent an over-long line".to_owned(),
                });
            }

            let filled = self.fill().await?;
            if filled == 0 {
                // End of stream. A trailing line without a terminator is
                // still returned; a clean close with nothing buffered is a
                // dropped connection.
                if self.start < self.buf.len() {
                    let line = self.buf[self.start..].to_vec();
                    self.start = self.buf.len();
                    return Ok(line);
                }
                return Err(MailError::Network {
                    message: "the server closed the connection".to_owned(),
                });
            }
        }
    }

    /// Reads one status line and classifies it as `+OK` or `-ERR`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Protocol`] when the line is neither indicator,
    /// and propagates read failures from [`read_line`](Self::read_line).
    pub(crate) async fn read_status(&mut self) -> Result<Reply, MailError> {
        let line = self.read_line().await?;
        let text = String::from_utf8_lossy(&line);
        let trimmed = text.trim_start();
        if let Some(rest) = indicator(trimmed, "+OK") {
            Ok(Reply::Ok(rest.trim_start().to_owned()))
        } else if indicator(trimmed, "-ERR").is_some() {
            Ok(Reply::Err)
        } else {
            Err(MailError::Protocol {
                message: "the server sent a malformed status line".to_owned(),
            })
        }
    }

    /// Reads a multi-line reply body, returning each content line with the
    /// POP3 dot-stuffing removed and the terminating `.` line consumed.
    ///
    /// # Errors
    ///
    /// Propagates read failures; a closed stream before the terminator
    /// surfaces as [`MailError::Network`].
    pub(crate) async fn read_multiline(&mut self) -> Result<Vec<Vec<u8>>, MailError> {
        let mut lines = Vec::new();
        loop {
            let mut line = self.read_line().await?;
            if line == b"." {
                return Ok(lines);
            }
            // Reverse the transparency stuffing: a leading '.' was doubled.
            if line.first() == Some(&b'.') {
                line.remove(0);
            }
            lines.push(line);
        }
    }

    /// Writes a command line followed by `CRLF` and flushes it.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Network`] on any write failure.
    pub(crate) async fn write_line(&mut self, line: &str) -> Result<(), MailError> {
        self.stream
            .write_all(line.as_bytes())
            .await
            .map_err(|err| map_io_error(&err))?;
        self.stream
            .write_all(b"\r\n")
            .await
            .map_err(|err| map_io_error(&err))?;
        self.stream.flush().await.map_err(|err| map_io_error(&err))
    }

    /// Reads more bytes into the buffer, returning how many arrived.
    async fn fill(&mut self) -> Result<usize, MailError> {
        let mut chunk = [0u8; READ_CHUNK];
        let read = self
            .stream
            .read(&mut chunk)
            .await
            .map_err(|err| map_io_error(&err))?;
        self.buf.extend_from_slice(&chunk[..read]);
        Ok(read)
    }

    /// Drops the consumed prefix once it grows past the threshold, keeping
    /// buffer reuse cheap without reallocating on every line.
    fn compact(&mut self) {
        if self.start >= COMPACT_THRESHOLD {
            self.buf.drain(..self.start);
            self.start = 0;
        }
    }
}

/// Returns the text after `prefix` when `line` begins with it (ASCII
/// case-insensitive) and the prefix is a whole token (end of line or a
/// following space).
fn indicator<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.get(..prefix.len())?;
    if !rest.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let after = &line[prefix.len()..];
    if after.is_empty() || after.starts_with(' ') {
        Some(after)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::io::Cursor;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    /// A read-only stream over a fixed byte script; writes are discarded.
    struct MockStream {
        reader: Cursor<Vec<u8>>,
    }

    impl MockStream {
        fn new(data: &[u8]) -> Self {
            Self {
                reader: Cursor::new(data.to_vec()),
            }
        }
    }

    impl AsyncRead for MockStream {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let remaining = self.reader.get_ref().len() as u64 - self.reader.position();
            if remaining == 0 {
                return std::task::Poll::Ready(Ok(()));
            }
            let pos = self.reader.position() as usize;
            let take = (buf.remaining()).min(remaining as usize);
            let data = self.reader.get_ref()[pos..pos + take].to_vec();
            buf.put_slice(&data);
            self.reader.set_position((pos + take) as u64);
            std::task::Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for MockStream {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Ok(buf.len()))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn reads_crlf_and_lf_lines() {
        let mut wire = Wire::new(MockStream::new(b"+OK ready\r\nnaked\n"));
        assert_eq!(wire.read_line().await.unwrap(), b"+OK ready");
        assert_eq!(wire.read_line().await.unwrap(), b"naked");
    }

    #[tokio::test]
    async fn status_ok_and_err_classified() {
        let mut wire = Wire::new(MockStream::new(b"+OK 3 320\r\n-ERR nope\r\n"));
        assert!(matches!(wire.read_status().await.unwrap(), Reply::Ok(rest) if rest == "3 320"));
        assert!(matches!(wire.read_status().await.unwrap(), Reply::Err));
    }

    #[tokio::test]
    async fn multiline_unstuffs_and_stops_at_dot() {
        let mut wire = Wire::new(MockStream::new(b"line one\r\n..dotted\r\n.\r\n"));
        let lines = wire.read_multiline().await.unwrap();
        assert_eq!(lines, vec![b"line one".to_vec(), b".dotted".to_vec()]);
    }

    #[tokio::test]
    async fn closed_stream_is_network_error() {
        let mut wire = Wire::new(MockStream::new(b""));
        assert!(matches!(
            wire.read_line().await,
            Err(MailError::Network { .. })
        ));
    }
}
