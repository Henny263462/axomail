//! Thin IMAP dialog builders.
//!
//! These produce the small, common scripts an IMAP client walks during
//! connect and login. They assume the client issues a conventional command
//! tag (for example `a1`); callers needing anything richer than
//! greeting/capabilities/login should hand-write a `Vec<`[`Step`]`>`.

use crate::script::Step;

/// Untagged greeting advertising a modest capability set.
const GREETING: &str =
    "* OK [CAPABILITY IMAP4rev1 STARTTLS AUTH=PLAIN LOGIN] axomail test server ready";

/// The untagged capability line replayed for a `CAPABILITY` command.
const CAPABILITIES: &str = "* CAPABILITY IMAP4rev1 IDLE UIDPLUS AUTH=PLAIN LOGIN";

/// Server greeting line, sent as soon as the connection opens.
#[must_use]
pub fn greeting() -> Step {
    Step::send_line(GREETING)
}

/// Answers a `CAPABILITY` command with a fixed capability list.
///
/// `tag` must match the tag the client used for the command.
#[must_use]
pub fn capability(tag: &str) -> Vec<Step> {
    vec![
        Step::expect_containing("CAPABILITY"),
        Step::send_line(CAPABILITIES),
        Step::send_line(format!("{tag} OK CAPABILITY completed")),
    ]
}

/// Greeting followed by an accepted `LOGIN`, tagged with `tag`.
#[must_use]
pub fn login_ok(tag: &str) -> Vec<Step> {
    vec![
        greeting(),
        Step::expect_containing("LOGIN"),
        Step::send_line(format!(
            "{tag} OK [CAPABILITY IMAP4rev1 IDLE UIDPLUS] LOGIN completed"
        )),
    ]
}

/// Greeting followed by a rejected `LOGIN`, tagged with `tag`.
#[must_use]
pub fn login_fail(tag: &str) -> Vec<Step> {
    vec![
        greeting(),
        Step::expect_containing("LOGIN"),
        Step::send_line(format!(
            "{tag} NO [AUTHENTICATIONFAILED] credentials rejected"
        )),
    ]
}

/// Accepts a `LOGOUT`, tagged with `tag`, then closes.
#[must_use]
pub fn logout(tag: &str) -> Vec<Step> {
    vec![
        Step::expect_containing("LOGOUT"),
        Step::send_line("* BYE axomail test server logging out"),
        Step::send_line(format!("{tag} OK LOGOUT completed")),
        Step::Close,
    ]
}
