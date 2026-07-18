//! Thin SMTP submission dialog builders.
//!
//! These cover the common ESMTP submission handshake: greeting, `EHLO`,
//! optional `AUTH`, and a `MAIL FROM`/`RCPT TO`/`DATA` exchange whose
//! message body is captured verbatim in the server transcript. Anything
//! more elaborate (multiple recipients, pipelining edge cases) is left to
//! hand-written scripts.

use crate::script::Step;

/// Service-ready greeting line.
const GREETING: &str = "220 axomail.test ESMTP ready";

/// Server greeting, sent as soon as the connection opens.
#[must_use]
pub fn greeting() -> Step {
    Step::send_line(GREETING)
}

/// Accepts an `EHLO` and advertises a small extension set.
#[must_use]
pub fn ehlo_ok() -> Vec<Step> {
    vec![
        Step::expect_prefix("EHLO"),
        Step::send_line("250-axomail.test greets you"),
        Step::send_line("250-8BITMIME"),
        Step::send_line("250-AUTH PLAIN LOGIN"),
        Step::send_line("250 SIZE 52428800"),
    ]
}

/// Accepts an `AUTH` command with a success reply.
#[must_use]
pub fn auth_ok() -> Vec<Step> {
    vec![
        Step::expect_prefix("AUTH"),
        Step::send_line("235 2.7.0 authentication succeeded"),
    ]
}

/// A `MAIL FROM`/`RCPT TO`/`DATA` exchange that captures the message body.
///
/// The body is consumed up to the `.<CRLF>` terminator and is recorded in
/// the connection transcript, so tests can assert on exactly what the
/// client transmitted. Only a single recipient is expected.
#[must_use]
pub fn mail_transaction() -> Vec<Step> {
    vec![
        Step::expect_prefix("MAIL FROM"),
        Step::send_line("250 2.1.0 sender ok"),
        Step::expect_prefix("RCPT TO"),
        Step::send_line("250 2.1.5 recipient ok"),
        Step::expect_prefix("DATA"),
        Step::send_line("354 end data with <CR><LF>.<CR><LF>"),
        Step::expect_until("."),
        Step::send_line("250 2.0.0 message accepted for delivery"),
    ]
}

/// Accepts a `QUIT` and closes the connection.
#[must_use]
pub fn quit() -> Vec<Step> {
    vec![
        Step::expect_prefix("QUIT"),
        Step::send_line("221 2.0.0 axomail.test closing connection"),
        Step::Close,
    ]
}

/// A full unauthenticated submission session: greeting, `EHLO`, one
/// message transaction, and `QUIT`.
#[must_use]
pub fn submission_session() -> Vec<Step> {
    let mut steps = vec![greeting()];
    steps.extend(ehlo_ok());
    steps.extend(mail_transaction());
    steps.extend(quit());
    steps
}
