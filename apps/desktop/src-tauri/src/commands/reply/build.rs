//! Pure assembly of a [`ComposerPrefillDto`] from a parsed source message.
//!
//! Everything here is store-free and deterministic: given a parsed message, the
//! mode, the account's addresses, and the (already resolved) sender identity,
//! it produces the recipients, subject, body, threading headers, and — for a
//! forward-as-attachment — the `message/rfc822` part. The command layer
//! (`super`) does the I/O (load the message, fetch the raw body, mark the
//! source) around this.

use std::collections::BTreeSet;

use axomail_core::Address;
use axomail_parser::ParsedMessage;
use axomail_security::sanitize_attachment_filename;
use axomail_storage::{ForwardMode, Identity, ReplyPosition, SignaturePlacement};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::Mode;
use super::quote::{assemble, forward_block, reply_quote, signature_block};
use super::recipients::derive_recipients;
use super::subject::{Prefix, normalize_subject};
use crate::dto::{ComposeAttachmentDto, ComposerPrefillDto, PrefillSourceDto, SourceMark};

/// The composition preferences a prefill honors, taken from the resolved
/// identity or a sensible default when there is none.
struct Prefs {
    reply_position: ReplyPosition,
    signature_placement: SignaturePlacement,
    forward_mode: ForwardMode,
    signature_text: Option<String>,
}

impl Prefs {
    /// Reads the preferences off an identity, or the account-seed defaults
    /// (bottom-posting, signature below the quote, inline forward) when no
    /// identity was resolved.
    fn resolve(identity: Option<&Identity>) -> Self {
        match identity {
            Some(identity) => Self {
                reply_position: identity.fields.reply_position,
                signature_placement: identity.fields.signature_placement,
                forward_mode: identity.fields.forward_mode,
                signature_text: identity.fields.signature_text.clone(),
            },
            None => Self {
                reply_position: ReplyPosition::Below,
                signature_placement: SignaturePlacement::BelowQuote,
                forward_mode: ForwardMode::Inline,
                signature_text: None,
            },
        }
    }
}

/// Builds the composer prefill for `mode`.
///
/// `source_message_id` is the local id of the parsed message (echoed back for
/// the post-send flag), `account_id` the account to compose from, `self_emails`
/// the account's own lowercased addresses (for reply-all self-exclusion), and
/// `raw_bytes` the source `.eml` (only used for a forward-as-attachment).
pub(super) fn build_prefill(
    parsed: &ParsedMessage,
    raw_bytes: &[u8],
    mode: Mode,
    account_id: i64,
    source_message_id: i64,
    identity: Option<&Identity>,
    self_emails: &BTreeSet<String>,
) -> ComposerPrefillDto {
    let prefs = Prefs::resolve(identity);
    let signature = signature_block(prefs.signature_text.as_deref());
    let recipients = derive_recipients(parsed, mode, self_emails);

    let (body, attachments, in_reply_to, references, mark, prefix) = match mode {
        Mode::Reply | Mode::ReplyAll => {
            let block = reply_quote(parsed);
            let body = assemble(
                prefs.reply_position,
                prefs.signature_placement,
                &block,
                &signature,
            );
            (
                body,
                Vec::new(),
                parsed.message_id.clone(),
                reply_references(parsed),
                SourceMark::Answered,
                Prefix::Reply,
            )
        }
        Mode::Forward => {
            let (block, attachments) = match prefs.forward_mode {
                ForwardMode::Inline => (forward_block(parsed), Vec::new()),
                ForwardMode::Attachment => {
                    (String::new(), vec![forwarded_attachment(parsed, raw_bytes)])
                }
            };
            let body = assemble(
                prefs.reply_position,
                prefs.signature_placement,
                &block,
                &signature,
            );
            (
                body,
                attachments,
                None,
                Vec::new(),
                SourceMark::Forwarded,
                Prefix::Forward,
            )
        }
    };

    ComposerPrefillDto {
        account_id,
        identity_id: identity.map(|identity| identity.id.value()),
        to: address_strings(&recipients.to),
        cc: address_strings(&recipients.cc),
        subject: normalize_subject(parsed.subject.as_deref(), prefix),
        body,
        in_reply_to,
        references,
        attachments,
        source: PrefillSourceDto {
            message_id: source_message_id,
            mark,
        },
    }
}

/// The account's own addresses (its email plus every identity's email),
/// lowercased for case-insensitive matching.
pub(super) fn self_email_set(account_email: &str, identities: &[Identity]) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    set.insert(account_email.to_ascii_lowercase());
    for identity in identities {
        set.insert(identity.fields.email.to_ascii_lowercase());
    }
    set
}

/// Guesses the sending identity by matching an identity's address against the
/// source message's delivered-to (its `To`/`Cc`). Falls back to the account
/// default, then the first identity.
pub(super) fn guess_identity<'a>(
    parsed: &ParsedMessage,
    identities: &'a [Identity],
) -> Option<&'a Identity> {
    let delivered_to: BTreeSet<String> = parsed
        .to
        .iter()
        .chain(&parsed.cc)
        .map(|address| address.email.as_str().to_ascii_lowercase())
        .collect();
    identities
        .iter()
        .find(|identity| delivered_to.contains(&identity.fields.email.to_ascii_lowercase()))
        .or_else(|| identities.iter().find(|identity| identity.is_default))
        .or_else(|| identities.first())
}

/// The reply's `References`: the source chain plus the source Message-ID.
fn reply_references(parsed: &ParsedMessage) -> Vec<String> {
    let mut references = parsed.references.clone();
    if let Some(message_id) = &parsed.message_id
        && references.last() != Some(message_id)
    {
        references.push(message_id.clone());
    }
    references
}

/// Builds the `message/rfc822` attachment carrying the forwarded source.
fn forwarded_attachment(parsed: &ParsedMessage, raw_bytes: &[u8]) -> ComposeAttachmentDto {
    ComposeAttachmentDto {
        filename: forwarded_filename(parsed.subject.as_deref()),
        mime_type: "message/rfc822".to_owned(),
        content_base64: STANDARD.encode(raw_bytes),
    }
}

/// A sanitized `.eml` filename for a forwarded message, derived from its
/// subject (or a fixed fallback for a subject-less message).
fn forwarded_filename(subject: Option<&str>) -> String {
    let base = subject.map(str::trim).filter(|s| !s.is_empty());
    let sanitized = base.map(sanitize_attachment_filename).unwrap_or_default();
    let stem = if sanitized.trim().is_empty() {
        "forwarded-message".to_owned()
    } else {
        sanitized
    };
    if stem.to_ascii_lowercase().ends_with(".eml") {
        stem
    } else {
        format!("{stem}.eml")
    }
}

/// Renders addresses to the plain `Name <email>` / `email` strings the
/// composer's recipient fields hold.
fn address_strings(addresses: &[Address]) -> Vec<String> {
    addresses
        .iter()
        .map(|address| match &address.display_name {
            Some(name) if !name.trim().is_empty() => {
                format!("{name} <{}>", address.email.as_str())
            }
            _ => address.email.as_str().to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axomail_core::{Address, EmailAddress, UtcTimestamp};

    use super::*;

    fn addr(name: Option<&str>, email: &str) -> Address {
        Address {
            display_name: name.map(str::to_owned),
            email: EmailAddress::parse(email).unwrap(),
        }
    }

    fn source() -> ParsedMessage {
        ParsedMessage {
            subject: Some("Weekly sync".to_owned()),
            from: vec![addr(Some("Ada Lovelace"), "ada@example.org")],
            to: vec![addr(None, "me@example.org"), addr(None, "bob@example.org")],
            cc: vec![addr(None, "carol@example.net")],
            reply_to: None,
            date: Some(UtcTimestamp::from_unix_seconds(1_600_000_000)),
            message_id: Some("parent@example.org".to_owned()),
            in_reply_to: Vec::new(),
            references: vec!["root@example.org".to_owned()],
            text_body: Some("Original line.".to_owned()),
            html_body: None,
            preview: String::new(),
            attachments: Vec::new(),
            disposition_notification_to: Vec::new(),
            mailing_list: false,
            parse_warnings: Vec::new(),
        }
    }

    fn self_set() -> BTreeSet<String> {
        self_email_set("me@example.org", &[])
    }

    #[test]
    fn reply_prefill_carries_threading_and_subject_and_answered_mark() {
        let out = build_prefill(&source(), &[], Mode::Reply, 7, 42, None, &self_set());
        assert_eq!(out.account_id, 7);
        assert_eq!(out.to, vec!["Ada Lovelace <ada@example.org>"]);
        assert!(out.cc.is_empty());
        assert_eq!(out.subject, "Re: Weekly sync");
        assert_eq!(out.in_reply_to.as_deref(), Some("parent@example.org"));
        // References = source chain + source Message-ID.
        assert_eq!(
            out.references,
            vec!["root@example.org", "parent@example.org"]
        );
        assert!(out.body.contains("> Original line."));
        assert!(out.attachments.is_empty());
        assert_eq!(out.source.message_id, 42);
        assert_eq!(out.source.mark, SourceMark::Answered);
    }

    #[test]
    fn reply_all_prefill_gathers_recipients_minus_self() {
        let out = build_prefill(&source(), &[], Mode::ReplyAll, 7, 42, None, &self_set());
        assert_eq!(
            out.to,
            vec!["Ada Lovelace <ada@example.org>", "bob@example.org"]
        );
        assert_eq!(out.cc, vec!["carol@example.net"]);
    }

    #[test]
    fn forward_inline_prefill_reproduces_the_original_and_marks_forwarded() {
        let out = build_prefill(&source(), b"raw", Mode::Forward, 7, 42, None, &self_set());
        assert!(out.to.is_empty());
        assert_eq!(out.subject, "Fwd: Weekly sync");
        assert!(out.in_reply_to.is_none());
        assert!(out.references.is_empty());
        assert!(out.body.contains("-------- Forwarded Message --------"));
        assert!(out.body.contains("Original line."));
        assert!(
            out.attachments.is_empty(),
            "inline forward has no attachment"
        );
        assert_eq!(out.source.mark, SourceMark::Forwarded);
    }

    #[test]
    fn guess_identity_matches_the_delivered_to_address() {
        let identities = vec![
            identity(1, "other@example.org", false),
            identity(2, "me@example.org", true),
        ];
        let chosen = guess_identity(&source(), &identities).unwrap();
        assert_eq!(chosen.id.value(), 2);
    }

    #[test]
    fn guess_identity_falls_back_to_default_when_no_address_matches() {
        let identities = vec![
            identity(1, "nomatch@example.org", false),
            identity(2, "default@example.org", true),
        ];
        let chosen = guess_identity(&source(), &identities).unwrap();
        assert_eq!(chosen.id.value(), 2, "default identity is the fallback");
    }

    #[test]
    fn forwarded_filename_sanitizes_and_appends_eml() {
        assert_eq!(forwarded_filename(Some("Weekly sync")), "Weekly sync.eml");
        assert_eq!(forwarded_filename(None), "forwarded-message.eml");
        // Path separators must not survive into the filename.
        assert!(!forwarded_filename(Some("../etc/passwd")).contains('/'));
    }

    fn identity(id: i64, email: &str, is_default: bool) -> Identity {
        use axomail_core::AccountId;
        use axomail_storage::{ArchiveGranularity, IdentityFields, IdentityId};
        Identity {
            id: IdentityId::new(id),
            account: AccountId::new(1),
            fields: IdentityFields {
                email: email.to_owned(),
                display_name: String::new(),
                reply_to: None,
                signature_text: None,
                signature_placement: SignaturePlacement::BelowQuote,
                reply_position: ReplyPosition::Below,
                forward_mode: ForwardMode::Inline,
                archive_granularity: ArchiveGranularity::Flat,
                sent_dest: None,
                drafts_dest: None,
                templates_dest: None,
                archive_dest: None,
            },
            is_default,
            sort_position: 0,
        }
    }
}
