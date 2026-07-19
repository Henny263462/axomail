//! Message envelope models.
//!
//! Envelopes describe a message for list views without carrying the
//! body. Full content is loaded on demand through the storage layer and
//! never lives in these types.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::id::{MailboxId, MessageId, RemoteMessageId};

/// A point in time as seconds since the Unix epoch (UTC).
///
/// Kept as a plain integer to avoid a date-time dependency in the core;
/// header parsing into this form is the job of `axomail-parser`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtcTimestamp(i64);

impl UtcTimestamp {
    /// Wraps seconds since the Unix epoch; negative values are valid
    /// and represent dates before 1970.
    #[must_use]
    pub const fn from_unix_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    /// Returns seconds since the Unix epoch.
    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.0
    }
}

/// Threading identity headers of a message (RFC 5322 §3.6.4).
///
/// These drive conversation grouping. `message_id` is the message's own
/// identity; `in_reply_to` and `references` point at ancestors. All ids are
/// stored without their surrounding angle brackets, matching the parser and
/// the on-disk columns (migration 0006). An all-empty value means the
/// message carries no threading headers (or they have not been extracted
/// yet — see the storage layer's `references_json IS NULL` sentinel).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadingHeaders {
    /// `Message-ID` without angle brackets, if the message declares one.
    pub message_id: Option<String>,
    /// `In-Reply-To` message-ids in header order. Usually exactly one
    /// parent, but the header may legitimately carry several.
    pub in_reply_to: Vec<String>,
    /// `References` chain, oldest ancestor first.
    pub references: Vec<String>,
}

impl ThreadingHeaders {
    /// Returns `true` when no threading header is present at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.message_id.is_none() && self.in_reply_to.is_empty() && self.references.is_empty()
    }

    /// The conventional immediate parent for thread assembly: the last id in
    /// `References` (the nearest ancestor) if present, otherwise the first
    /// `In-Reply-To` id. `None` when the message references no parent.
    #[must_use]
    pub fn parent_id(&self) -> Option<&str> {
        self.references
            .last()
            .or_else(|| self.in_reply_to.first())
            .map(String::as_str)
    }
}

/// Standard per-message flags, aligned with IMAP system flags but used
/// protocol-independently (POP3 messages carry purely local flags).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageFlags {
    /// Message has been read.
    pub seen: bool,
    /// Message has been answered.
    pub answered: bool,
    /// Message is flagged/starred.
    pub flagged: bool,
    /// Message is an unsent draft.
    pub draft: bool,
    /// Message is marked for deletion.
    pub deleted: bool,
}

/// Summary of a message for list rendering and sync bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// Local identifier.
    pub id: MessageId,
    /// Mailbox the message currently lives in.
    pub mailbox: MailboxId,
    /// Identity on the remote server; `None` for local-only messages
    /// such as unsent drafts.
    pub remote_id: Option<RemoteMessageId>,
    /// Decoded subject, if present.
    pub subject: Option<String>,
    /// Senders (usually exactly one).
    pub from: Vec<Address>,
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Date header, if present and parseable.
    pub date: Option<UtcTimestamp>,
    /// Current flags.
    pub flags: MessageFlags,
    /// Full message size in bytes, if known.
    pub size_bytes: Option<u64>,
    /// Short plain-text preview (already sanitized, no markup).
    pub preview: Option<String>,
    /// Threading identity (Message-ID / In-Reply-To / References). Empty
    /// until the columns have been populated at ingest or backfilled on
    /// first open (migration 0006).
    pub threading: ThreadingHeaders,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::address::EmailAddress;

    #[test]
    fn timestamp_round_trips_and_orders() {
        let early = UtcTimestamp::from_unix_seconds(-1);
        let late = UtcTimestamp::from_unix_seconds(1_752_800_000);
        assert!(early < late);
        assert_eq!(late.unix_seconds(), 1_752_800_000);
    }

    #[test]
    fn default_flags_are_all_clear() {
        let flags = MessageFlags::default();
        assert!(!flags.seen && !flags.answered && !flags.flagged && !flags.draft && !flags.deleted);
    }

    #[test]
    fn envelope_serializes_with_optional_fields_absent() {
        let envelope = MessageEnvelope {
            id: MessageId::new(10),
            mailbox: MailboxId::new(2),
            remote_id: None,
            subject: None,
            from: vec![Address::from_email(
                EmailAddress::parse("a@example.org").unwrap(),
            )],
            to: vec![],
            date: None,
            flags: MessageFlags::default(),
            size_bytes: None,
            preview: None,
            threading: ThreadingHeaders::default(),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"subject\":null"));
    }

    #[test]
    fn threading_parent_prefers_last_reference_then_in_reply_to() {
        let mut headers = ThreadingHeaders::default();
        assert!(headers.is_empty());
        assert_eq!(headers.parent_id(), None);

        headers.in_reply_to = vec!["a@x".to_owned()];
        assert!(!headers.is_empty());
        assert_eq!(headers.parent_id(), Some("a@x"));

        headers.references = vec!["root@x".to_owned(), "mid@x".to_owned()];
        assert_eq!(headers.parent_id(), Some("mid@x"));
    }
}
