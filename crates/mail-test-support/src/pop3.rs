//! Thin POP3 dialog builders over a configurable message set.
//!
//! The message set is a slice of raw RFC 5322 message bodies. Builders turn
//! it into the canonical download dialog a client walks: greeting,
//! `USER`/`PASS`, `STAT`, `LIST`, `UIDL`, one `RETR` per message, and
//! `QUIT`. Multi-line responses are correctly dot-stuffed and terminated,
//! so a naive client can read them line by line until the `.` terminator.

use crate::script::Step;

/// Service-ready greeting line.
const GREETING: &str = "+OK axomail POP3 test server ready";

/// Server greeting, sent as soon as the connection opens.
#[must_use]
pub fn greeting() -> Step {
    Step::send_line(GREETING)
}

/// Accepts `USER` then `PASS`, both with success replies.
#[must_use]
pub fn user_pass_ok() -> Vec<Step> {
    vec![
        Step::expect_prefix("USER"),
        Step::send_line("+OK user accepted"),
        Step::expect_prefix("PASS"),
        Step::send_line("+OK mailbox ready"),
    ]
}

/// Answers `STAT` with the message count and total octet size.
#[must_use]
pub fn stat(messages: &[&str]) -> Vec<Step> {
    vec![
        Step::expect_prefix("STAT"),
        Step::send_line(format!("+OK {} {}", messages.len(), total_octets(messages))),
    ]
}

/// Answers `LIST` with a scan listing, one `index size` line per message.
#[must_use]
pub fn list(messages: &[&str]) -> Vec<Step> {
    let lines: Vec<String> = messages
        .iter()
        .enumerate()
        .map(|(index, body)| format!("{} {}", index + 1, body.len()))
        .collect();
    vec![
        Step::expect_prefix("LIST"),
        multiline(&format!("+OK {} messages", messages.len()), &lines),
    ]
}

/// Answers `UIDL` with a deterministic unique id per message.
#[must_use]
pub fn uidl(messages: &[&str]) -> Vec<Step> {
    let lines: Vec<String> = (0..messages.len())
        .map(|index| format!("{} {}", index + 1, unique_id(index + 1)))
        .collect();
    vec![
        Step::expect_prefix("UIDL"),
        multiline("+OK unique-id listing follows", &lines),
    ]
}

/// Answers a single `RETR` for the given one-based sequence number.
///
/// The expectation is pinned to `RETR {sequence}`, so the dialog enforces
/// that the client retrieves messages in order.
#[must_use]
pub fn retr(sequence: usize, body: &str) -> Vec<Step> {
    vec![
        Step::expect_prefix(format!("RETR {sequence}")),
        Step::send_line(format!("+OK {} octets", body.len())),
        Step::send_raw(dot_stuffed(body)),
    ]
}

/// Answers a single `DELE` for the given one-based sequence number.
#[must_use]
pub fn dele(sequence: usize) -> Vec<Step> {
    vec![
        Step::expect_prefix(format!("DELE {sequence}")),
        Step::send_line(format!("+OK message {sequence} deleted")),
    ]
}

/// Accepts `QUIT` and closes the connection.
#[must_use]
pub fn quit() -> Vec<Step> {
    vec![
        Step::expect_prefix("QUIT"),
        Step::send_line("+OK axomail POP3 test server signing off"),
        Step::Close,
    ]
}

/// The canonical read-only download dialog for `messages`.
///
/// Order: greeting, `USER`, `PASS`, `STAT`, `LIST`, `UIDL`, one `RETR` per
/// message in sequence, then `QUIT`.
#[must_use]
pub fn download_session(messages: &[&str]) -> Vec<Step> {
    let mut steps = vec![greeting()];
    steps.extend(user_pass_ok());
    steps.extend(stat(messages));
    steps.extend(list(messages));
    steps.extend(uidl(messages));
    for (index, body) in messages.iter().enumerate() {
        steps.extend(retr(index + 1, body));
    }
    steps.extend(quit());
    steps
}

/// Like [`download_session`] but issues a `DELE` after each `RETR`, matching
/// the classic download-and-delete POP3 flow.
#[must_use]
pub fn download_and_delete_session(messages: &[&str]) -> Vec<Step> {
    let mut steps = vec![greeting()];
    steps.extend(user_pass_ok());
    steps.extend(stat(messages));
    for (index, body) in messages.iter().enumerate() {
        steps.extend(retr(index + 1, body));
        steps.extend(dele(index + 1));
    }
    steps.extend(quit());
    steps
}

/// Sum of message octet counts, used by `STAT`.
fn total_octets(messages: &[&str]) -> usize {
    messages.iter().map(|body| body.len()).sum()
}

/// Deterministic pseudo-unique id for `UIDL`.
fn unique_id(sequence: usize) -> String {
    format!("axomail{sequence:08}")
}

/// Builds a dot-terminated multi-line response from an intro line and body
/// lines, as a single raw write.
fn multiline(intro: &str, lines: &[String]) -> Step {
    let mut out = String::new();
    out.push_str(intro);
    out.push_str("\r\n");
    for line in lines {
        push_dot_stuffed_line(&mut out, line);
    }
    out.push_str(".\r\n");
    Step::send_raw(out.into_bytes())
}

/// Encodes a message body as a POP3 multi-line data block: CRLF-normalized,
/// dot-stuffed, and terminated by `.<CRLF>`.
fn dot_stuffed(body: &str) -> Vec<u8> {
    let mut out = String::new();
    for line in split_lines(body) {
        push_dot_stuffed_line(&mut out, line);
    }
    out.push_str(".\r\n");
    out.into_bytes()
}

/// Appends one line, adding the leading-dot byte-stuffing POP3 requires and
/// a trailing CRLF.
fn push_dot_stuffed_line(out: &mut String, line: &str) {
    if line.starts_with('.') {
        out.push('.');
    }
    out.push_str(line);
    out.push_str("\r\n");
}

/// Splits a body into lines, tolerating both `\r\n` and bare `\n` and
/// dropping a single trailing empty line so bodies do not gain a spurious
/// blank line before the terminator.
fn split_lines(body: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = body
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}
