//! The POP3 command verbs, built on the [`Wire`] framing layer.
//!
//! Each function performs one full command exchange (write plus read) under
//! a single deadline. Mandatory commands map a `-ERR` reply to a
//! [`MailError`]; optional commands (`CAPA`, `UIDL`, `TOP`) instead report
//! their absence as `None`, so higher layers can degrade gracefully.
//!
//! Server reply text is never copied into errors: only numeric fields are
//! parsed out of `+OK` lines, and `-ERR` text is discarded by the framing
//! layer (see [`Wire::read_status`]).

use std::time::Duration;

use axomail_core::{MailError, xoauth2_sasl};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::with_deadline;
use crate::wire::{Reply, Wire};

/// Sends `CAPA` and, if supported, returns the advertised capability atoms
/// (the first, upper-cased token of each line). `None` means the server
/// answered `-ERR`, which is tolerated: capabilities are then unknown.
pub(crate) async fn capabilities<S>(
    wire: &mut Wire<S>,
    timeout: Duration,
) -> Result<Option<Vec<String>>, MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("CAPA").await?;
        match wire.read_status().await? {
            Reply::Ok(_) => {
                let lines = wire.read_multiline().await?;
                let atoms = lines
                    .iter()
                    .filter_map(|line| first_token(line))
                    .map(|token| token.to_ascii_uppercase())
                    .collect();
                Ok(Some(atoms))
            }
            Reply::Err => Ok(None),
        }
    })
    .await
}

/// Requests a `STLS` upgrade. A `-ERR` reply means the server does not
/// support it, mapped to [`MailError::UnsupportedCapability`].
pub(crate) async fn stls<S>(wire: &mut Wire<S>, timeout: Duration) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("STLS").await?;
        match wire.read_status().await? {
            Reply::Ok(_) => Ok(()),
            Reply::Err => Err(MailError::UnsupportedCapability {
                capability: "STLS".to_owned(),
            }),
        }
    })
    .await
}

/// Authenticates with `USER`/`PASS`. Either rejection becomes a
/// payload-free [`MailError::Authentication`], so nothing the server echoed
/// is retained.
pub(crate) async fn authenticate<S>(
    wire: &mut Wire<S>,
    username: &str,
    password: &str,
    timeout: Duration,
) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line(&format!("USER {username}")).await?;
        if matches!(wire.read_status().await?, Reply::Err) {
            return Err(MailError::Authentication);
        }
        wire.write_line(&format!("PASS {password}")).await?;
        if matches!(wire.read_status().await?, Reply::Err) {
            return Err(MailError::Authentication);
        }
        Ok(())
    })
    .await
}

/// Authenticates with the SASL `XOAUTH2` mechanism (RFC 5034 + Google/
/// Microsoft XOAUTH2) using the initial-response form
/// `AUTH XOAUTH2 <base64>`.
///
/// A `+OK` reply means success. Any other reply — including the
/// `+ <base64 error>` SASL continuation providers send on failure — is treated
/// as an authentication failure; when a continuation arrives the exchange is
/// finished with an empty line so the server can emit its final `-ERR` before
/// the session is torn down. No server-supplied text is ever retained.
pub(crate) async fn authenticate_xoauth2<S>(
    wire: &mut Wire<S>,
    username: &str,
    access_token: &str,
    timeout: Duration,
) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        let encoded = BASE64.encode(xoauth2_sasl(username, access_token).as_bytes());
        wire.write_line(&format!("AUTH XOAUTH2 {encoded}")).await?;
        let line = wire.read_line().await?;
        let text = String::from_utf8_lossy(&line);
        let trimmed = text.trim_start();
        if trimmed
            .get(..3)
            .is_some_and(|p| p.eq_ignore_ascii_case("+OK"))
        {
            return Ok(());
        }
        // A SASL continuation (`+ ...`, but not `+OK`) needs an empty line to
        // let the server finish and report its terminal `-ERR`.
        if trimmed.starts_with('+') {
            wire.write_line("").await?;
            let _ = wire.read_line().await;
        }
        Err(MailError::Authentication)
    })
    .await
}

/// Issues `STAT`, returning the message count and total octet size.
pub(crate) async fn stat<S>(wire: &mut Wire<S>, timeout: Duration) -> Result<(u32, u64), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("STAT").await?;
        match wire.read_status().await? {
            Reply::Ok(rest) => parse_two_numbers(&rest).ok_or_else(|| MailError::Protocol {
                message: "the server sent a malformed STAT reply".to_owned(),
            }),
            Reply::Err => Err(MailError::Protocol {
                message: "the server refused STAT".to_owned(),
            }),
        }
    })
    .await
}

/// Issues `LIST` (all messages), returning `(number, size)` pairs.
pub(crate) async fn list_all<S>(
    wire: &mut Wire<S>,
    timeout: Duration,
) -> Result<Vec<(u32, u64)>, MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("LIST").await?;
        match wire.read_status().await? {
            Reply::Ok(_) => {
                let lines = wire.read_multiline().await?;
                Ok(lines
                    .iter()
                    .filter_map(|line| parse_list_line(line))
                    .collect())
            }
            Reply::Err => Err(MailError::Protocol {
                message: "the server refused LIST".to_owned(),
            }),
        }
    })
    .await
}

/// Issues `UIDL` (all messages). `None` means the server answered `-ERR`,
/// i.e. it does not support unique-id listing.
pub(crate) async fn uidl_all<S>(
    wire: &mut Wire<S>,
    timeout: Duration,
) -> Result<Option<Vec<(u32, String)>>, MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("UIDL").await?;
        match wire.read_status().await? {
            Reply::Ok(_) => {
                let lines = wire.read_multiline().await?;
                Ok(Some(
                    lines
                        .iter()
                        .filter_map(|line| parse_uidl_line(line))
                        .collect(),
                ))
            }
            Reply::Err => Ok(None),
        }
    })
    .await
}

/// Issues `TOP number lines`, returning the returned header/body bytes.
/// `None` means the server answered `-ERR`, i.e. it does not support `TOP`.
pub(crate) async fn top<S>(
    wire: &mut Wire<S>,
    number: u32,
    lines: u32,
    timeout: Duration,
) -> Result<Option<Vec<u8>>, MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line(&format!("TOP {number} {lines}")).await?;
        match wire.read_status().await? {
            Reply::Ok(_) => Ok(Some(join_body(wire.read_multiline().await?))),
            Reply::Err => Ok(None),
        }
    })
    .await
}

/// Issues `RETR number`, returning the full message bytes.
pub(crate) async fn retr<S>(
    wire: &mut Wire<S>,
    number: u32,
    timeout: Duration,
) -> Result<Vec<u8>, MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line(&format!("RETR {number}")).await?;
        match wire.read_status().await? {
            Reply::Ok(_) => Ok(join_body(wire.read_multiline().await?)),
            Reply::Err => Err(MailError::Protocol {
                message: "the server refused RETR for the requested message".to_owned(),
            }),
        }
    })
    .await
}

/// Issues `DELE number`, marking the message for deletion at `QUIT`.
pub(crate) async fn dele<S>(
    wire: &mut Wire<S>,
    number: u32,
    timeout: Duration,
) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line(&format!("DELE {number}")).await?;
        match wire.read_status().await? {
            Reply::Ok(_) => Ok(()),
            Reply::Err => Err(MailError::Protocol {
                message: "the server refused DELE for the requested message".to_owned(),
            }),
        }
    })
    .await
}

/// Issues `QUIT`, committing any pending `DELE`s. The server reply is read
/// but its status is not treated as fatal, since the session is closing.
pub(crate) async fn quit<S>(wire: &mut Wire<S>, timeout: Duration) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        wire.write_line("QUIT").await?;
        let _ = wire.read_status().await?;
        Ok(())
    })
    .await
}

/// Rejoins un-stuffed content lines into a single `CRLF`-terminated byte
/// stream, reconstructing the original message text.
fn join_body(lines: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        out.extend_from_slice(&line);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Returns the first whitespace-delimited token of a line, if any.
fn first_token(line: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(line);
    text.split_whitespace().next().map(str::to_owned)
}

/// Parses the leading two whitespace-separated integers of a reply body.
fn parse_two_numbers(rest: &str) -> Option<(u32, u64)> {
    let mut parts = rest.split_whitespace();
    let count: u32 = parts.next()?.parse().ok()?;
    let size: u64 = parts.next()?.parse().ok()?;
    Some((count, size))
}

/// Parses a `LIST` scan line: `number octets`.
fn parse_list_line(line: &[u8]) -> Option<(u32, u64)> {
    let text = String::from_utf8_lossy(line);
    let mut parts = text.split_whitespace();
    let number: u32 = parts.next()?.parse().ok()?;
    let size: u64 = parts.next()?.parse().ok()?;
    Some((number, size))
}

/// Parses a `UIDL` line: `number unique-id`.
fn parse_uidl_line(line: &[u8]) -> Option<(u32, String)> {
    let text = String::from_utf8_lossy(line);
    let mut parts = text.split_whitespace();
    let number: u32 = parts.next()?.parse().ok()?;
    let uid = parts.next()?.to_owned();
    Some((number, uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_body_reconstructs_crlf_stream() {
        let lines = vec![b"Subject: Hi".to_vec(), Vec::new(), b"Body".to_vec()];
        assert_eq!(join_body(lines), b"Subject: Hi\r\n\r\nBody\r\n");
    }

    #[test]
    fn parses_stat_and_scan_lines() {
        assert_eq!(parse_two_numbers("3 1200 extra"), Some((3, 1200)));
        assert_eq!(parse_list_line(b"2 512"), Some((2, 512)));
        assert_eq!(
            parse_uidl_line(b"7 whqtswO00WBw418f9t5JxYwZ"),
            Some((7, "whqtswO00WBw418f9t5JxYwZ".to_owned()))
        );
        assert_eq!(parse_two_numbers("garbage"), None);
    }

    #[test]
    fn first_token_extracts_capability_atom() {
        assert_eq!(first_token(b"SASL PLAIN LOGIN"), Some("SASL".to_owned()));
        assert_eq!(first_token(b"   "), None);
    }
}
