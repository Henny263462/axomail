//! A small scripted-dialog engine for line-oriented text protocols.
//!
//! A dialog is a `Vec<`[`Step`]`>` describing what the server expects to
//! read and what it sends back, in order. [`run_script`] drives one such
//! dialog over any async byte stream (plain TCP or TLS) and records a full
//! [`Transcript`] of every byte read and written for later assertions.
//!
//! Matching is deliberately tolerant about line endings: expectations are
//! evaluated against each inbound line with its trailing `\r\n` (or bare
//! `\n`) stripped, so a client may terminate lines either way.

use std::sync::{Mutex, PoisonError};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::error::TestServerError;

/// One instruction in a scripted dialog.
///
/// The first three variants consume exactly one inbound line each and fail
/// the dialog if it does not match; [`ExpectUntil`](Step::ExpectUntil)
/// consumes and records every line up to and including a terminator, which
/// is how variable-length payloads such as SMTP `DATA` bodies are captured.
#[derive(Debug, Clone)]
pub enum Step {
    /// Read one line and require it to contain the given substring.
    ExpectContaining(String),
    /// Read one line and require it to start with the given prefix.
    ExpectPrefix(String),
    /// Read and record lines until one equals the given terminator
    /// (compared after trimming the line ending); the terminator line is
    /// consumed. End-of-stream also stops the capture.
    ExpectUntil(String),
    /// Write the given text followed by a `\r\n` line terminator.
    SendLine(String),
    /// Write the given bytes verbatim, with no added terminator.
    SendRaw(Vec<u8>),
    /// Pause for the given number of milliseconds before the next step.
    StallMs(u64),
    /// Shut down the write half and end the dialog.
    Close,
}

impl Step {
    /// Builds an [`ExpectContaining`](Step::ExpectContaining) step.
    pub fn expect_containing(needle: impl Into<String>) -> Self {
        Self::ExpectContaining(needle.into())
    }

    /// Builds an [`ExpectPrefix`](Step::ExpectPrefix) step.
    pub fn expect_prefix(prefix: impl Into<String>) -> Self {
        Self::ExpectPrefix(prefix.into())
    }

    /// Builds an [`ExpectUntil`](Step::ExpectUntil) step.
    pub fn expect_until(terminator: impl Into<String>) -> Self {
        Self::ExpectUntil(terminator.into())
    }

    /// Builds a [`SendLine`](Step::SendLine) step.
    pub fn send_line(text: impl Into<String>) -> Self {
        Self::SendLine(text.into())
    }

    /// Builds a [`SendRaw`](Step::SendRaw) step.
    pub fn send_raw(bytes: impl Into<Vec<u8>>) -> Self {
        Self::SendRaw(bytes.into())
    }

    /// Builds a [`StallMs`](Step::StallMs) step.
    #[must_use]
    pub fn stall_ms(millis: u64) -> Self {
        Self::StallMs(millis)
    }
}

/// A recording of one connection's traffic and progress.
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    /// Every byte received from the peer, in order.
    pub received: Vec<u8>,
    /// Every byte sent to the peer, in order.
    pub sent: Vec<u8>,
    /// Number of script steps completed successfully.
    pub steps_completed: usize,
    /// Set when a step failed; otherwise `None`.
    pub error: Option<String>,
}

impl Transcript {
    /// Lossy UTF-8 view of everything received from the peer.
    #[must_use]
    pub fn received_text(&self) -> String {
        String::from_utf8_lossy(&self.received).into_owned()
    }

    /// Lossy UTF-8 view of everything sent to the peer.
    #[must_use]
    pub fn sent_text(&self) -> String {
        String::from_utf8_lossy(&self.sent).into_owned()
    }
}

/// Locks a mutex, recovering the guard even if a previous holder panicked.
///
/// Poison recovery keeps a single failed connection task from cascading
/// into every later assertion on the shared transcript.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Drives a scripted dialog to completion over one stream.
///
/// Progress and traffic are written into `sink` as the dialog runs, so a
/// caller holding the same `sink` can observe it live. On a failed
/// expectation the function returns [`TestServerError::Script`]; the caller
/// is responsible for recording that error into the transcript.
///
/// # Errors
///
/// Returns [`TestServerError::Io`] on stream failure and
/// [`TestServerError::Script`] when an expectation is not met.
pub(crate) async fn run_script<S>(
    stream: S,
    steps: &[Step],
    sink: &Mutex<Transcript>,
) -> Result<(), TestServerError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);

    for step in steps {
        match step {
            Step::ExpectContaining(needle) => {
                let line = expect_line(&mut reader, sink).await?;
                if !line.contains(needle.as_str()) {
                    return Err(TestServerError::Script(format!(
                        "expected a line containing {needle:?}, got {line:?}"
                    )));
                }
            }
            Step::ExpectPrefix(prefix) => {
                let line = expect_line(&mut reader, sink).await?;
                if !line.starts_with(prefix.as_str()) {
                    return Err(TestServerError::Script(format!(
                        "expected a line starting with {prefix:?}, got {line:?}"
                    )));
                }
            }
            Step::ExpectUntil(terminator) => {
                while let Some(line) = read_line(&mut reader, sink).await? {
                    if line == *terminator {
                        break;
                    }
                }
            }
            Step::SendLine(text) => {
                let mut bytes = text.clone().into_bytes();
                bytes.extend_from_slice(b"\r\n");
                write_all(&mut write_half, sink, &bytes).await?;
            }
            Step::SendRaw(bytes) => {
                write_all(&mut write_half, sink, bytes).await?;
            }
            Step::StallMs(millis) => {
                tokio::time::sleep(std::time::Duration::from_millis(*millis)).await;
            }
            Step::Close => {
                let _ = write_half.shutdown().await;
                lock(sink).steps_completed += 1;
                return Ok(());
            }
        }
        lock(sink).steps_completed += 1;
    }

    let _ = write_half.shutdown().await;
    Ok(())
}

/// Reads one line, requiring the stream not to end first.
async fn expect_line<R>(reader: &mut R, sink: &Mutex<Transcript>) -> Result<String, TestServerError>
where
    R: AsyncBufReadExt + Unpin,
{
    match read_line(reader, sink).await? {
        Some(line) => Ok(line),
        None => Err(TestServerError::Script(
            "connection closed while a line was expected".to_owned(),
        )),
    }
}

/// Reads one line, records its raw bytes, and returns it trimmed of the
/// trailing line ending. Returns `None` at end of stream.
async fn read_line<R>(
    reader: &mut R,
    sink: &Mutex<Transcript>,
) -> Result<Option<String>, TestServerError>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut raw = Vec::new();
    let read = reader.read_until(b'\n', &mut raw).await?;
    if read == 0 {
        return Ok(None);
    }
    lock(sink).received.extend_from_slice(&raw);
    Ok(Some(trim_line_ending(&raw)))
}

/// Writes all bytes, flushes, and records them in the transcript.
async fn write_all<W>(
    writer: &mut W,
    sink: &Mutex<Transcript>,
    bytes: &[u8],
) -> Result<(), TestServerError>
where
    W: AsyncWriteExt + Unpin,
{
    writer.write_all(bytes).await?;
    writer.flush().await?;
    lock(sink).sent.extend_from_slice(bytes);
    Ok(())
}

/// Trims a trailing `\n` and any preceding `\r`, returning a lossy string.
fn trim_line_ending(raw: &[u8]) -> String {
    let mut end = raw.len();
    if end > 0 && raw[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && raw[end - 1] == b'\r' {
            end -= 1;
        }
    }
    String::from_utf8_lossy(&raw[..end]).into_owned()
}
