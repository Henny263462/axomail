//! Recipient derivation for replies and forwards.
//!
//! * **Reply** → `To` is the Reply-To address, or the From address when there
//!   is no Reply-To. No Cc.
//! * **Reply-all** → `To` is the reply target plus every original `To`
//!   recipient; `Cc` is every original `Cc` recipient. The account's own
//!   identities are removed, and an address is never listed twice (dedup keeps
//!   the first occurrence and its display name, and never repeats a `To`
//!   recipient in `Cc`).
//! * **Forward** → no recipients (the user addresses it themselves).

use std::collections::BTreeSet;

use axomail_core::Address;
use axomail_parser::ParsedMessage;

use super::Mode;

/// The `To`/`Cc` lists a reply or forward starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DerivedRecipients {
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Carbon-copy recipients.
    pub cc: Vec<Address>,
}

/// Derives the initial recipients for `mode` from a parsed source message.
///
/// `self_emails` is the set of the account's own addresses (its email plus
/// every identity's email), **already lowercased**; those are excluded from a
/// reply-all so the sender never mails themselves.
pub(crate) fn derive_recipients(
    parsed: &ParsedMessage,
    mode: Mode,
    self_emails: &BTreeSet<String>,
) -> DerivedRecipients {
    match mode {
        Mode::Reply => DerivedRecipients {
            to: reply_target(parsed).into_iter().collect(),
            cc: Vec::new(),
        },
        Mode::ReplyAll => reply_all(parsed, self_emails),
        Mode::Forward => DerivedRecipients {
            to: Vec::new(),
            cc: Vec::new(),
        },
    }
}

/// Reply-all: reply target + original To in `To`, original Cc in `Cc`, minus
/// self, deduplicated across both lists.
fn reply_all(parsed: &ParsedMessage, self_emails: &BTreeSet<String>) -> DerivedRecipients {
    // Seed the "seen" set with the account's own addresses so they are dropped
    // from both lists; every address added is then remembered to dedup the rest.
    let mut seen: BTreeSet<String> = self_emails.clone();
    let mut to = Vec::new();
    if let Some(target) = reply_target(parsed) {
        push_unique(&mut to, &mut seen, target);
    }
    for address in &parsed.to {
        push_unique(&mut to, &mut seen, address.clone());
    }
    let mut cc = Vec::new();
    for address in &parsed.cc {
        push_unique(&mut cc, &mut seen, address.clone());
    }
    DerivedRecipients { to, cc }
}

/// The single address a plain reply is sent to: Reply-To if present, else the
/// first From address.
fn reply_target(parsed: &ParsedMessage) -> Option<Address> {
    parsed
        .reply_to
        .clone()
        .or_else(|| parsed.from.first().cloned())
}

/// Appends `address` unless its (lowercased) email has already been seen.
fn push_unique(list: &mut Vec<Address>, seen: &mut BTreeSet<String>, address: Address) {
    let key = address.email.as_str().to_ascii_lowercase();
    if seen.insert(key) {
        list.push(address);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axomail_core::{Address, EmailAddress, UtcTimestamp};

    use super::*;

    fn addr(email: &str) -> Address {
        Address::from_email(EmailAddress::parse(email).unwrap())
    }

    fn named(name: &str, email: &str) -> Address {
        Address {
            display_name: Some(name.to_owned()),
            email: EmailAddress::parse(email).unwrap(),
        }
    }

    /// A parsed message with the given envelope addresses; bodies are empty.
    fn message(
        from: Vec<Address>,
        to: Vec<Address>,
        cc: Vec<Address>,
        reply_to: Option<Address>,
    ) -> ParsedMessage {
        ParsedMessage {
            subject: Some("Subject".to_owned()),
            from,
            to,
            cc,
            reply_to,
            date: Some(UtcTimestamp::from_unix_seconds(0)),
            message_id: Some("m1@example.org".to_owned()),
            in_reply_to: Vec::new(),
            references: Vec::new(),
            text_body: Some("body".to_owned()),
            html_body: None,
            preview: String::new(),
            attachments: Vec::new(),
            disposition_notification_to: Vec::new(),
            mailing_list: false,
            parse_warnings: Vec::new(),
        }
    }

    fn self_set(emails: &[&str]) -> BTreeSet<String> {
        emails.iter().map(|e| e.to_ascii_lowercase()).collect()
    }

    #[test]
    fn reply_targets_from_when_no_reply_to() {
        let msg = message(
            vec![named("Ada", "ada@example.org")],
            vec![addr("me@x.org")],
            vec![],
            None,
        );
        let out = derive_recipients(&msg, Mode::Reply, &self_set(&["me@x.org"]));
        assert_eq!(out.to, vec![named("Ada", "ada@example.org")]);
        assert!(out.cc.is_empty());
    }

    #[test]
    fn reply_prefers_reply_to_over_from() {
        let msg = message(
            vec![addr("ada@example.org")],
            vec![addr("me@x.org")],
            vec![],
            Some(addr("list@example.org")),
        );
        let out = derive_recipients(&msg, Mode::Reply, &self_set(&["me@x.org"]));
        assert_eq!(out.to, vec![addr("list@example.org")]);
    }

    #[test]
    fn reply_all_includes_original_to_and_cc_minus_self() {
        let msg = message(
            vec![named("Ada", "ada@example.org")],
            vec![addr("me@x.org"), addr("bob@example.org")],
            vec![addr("carol@example.net")],
            None,
        );
        let out = derive_recipients(&msg, Mode::ReplyAll, &self_set(&["me@x.org"]));
        // Sender first, then the other To recipient; self is dropped.
        assert_eq!(
            out.to,
            vec![named("Ada", "ada@example.org"), addr("bob@example.org")]
        );
        assert_eq!(out.cc, vec![addr("carol@example.net")]);
    }

    #[test]
    fn reply_all_self_match_is_case_insensitive() {
        let msg = message(
            vec![addr("ada@example.org")],
            vec![addr("ME@X.org")],
            vec![],
            None,
        );
        let out = derive_recipients(&msg, Mode::ReplyAll, &self_set(&["me@x.org"]));
        assert_eq!(out.to, vec![addr("ada@example.org")]);
    }

    #[test]
    fn reply_all_deduplicates_across_to_and_cc() {
        let msg = message(
            vec![addr("ada@example.org")],
            vec![addr("bob@example.org")],
            // Ada and Bob repeated in Cc, plus a case-variant of Bob.
            vec![
                addr("ada@example.org"),
                addr("BOB@example.org"),
                addr("dan@example.net"),
            ],
            None,
        );
        let out = derive_recipients(&msg, Mode::ReplyAll, &self_set(&["me@x.org"]));
        assert_eq!(
            out.to,
            vec![addr("ada@example.org"), addr("bob@example.org")]
        );
        // Only the not-yet-seen dan@ survives into Cc.
        assert_eq!(out.cc, vec![addr("dan@example.net")]);
    }

    #[test]
    fn forward_has_no_recipients() {
        let msg = message(
            vec![addr("ada@example.org")],
            vec![addr("me@x.org")],
            vec![],
            None,
        );
        let out = derive_recipients(&msg, Mode::Forward, &self_set(&["me@x.org"]));
        assert!(out.to.is_empty());
        assert!(out.cc.is_empty());
    }
}
