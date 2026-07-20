//! Request/response types for the provider traits.
//!
//! These shapes keep protocol adapters stateless with respect to local
//! storage: the sync engine passes in the last known remote state and
//! persists whatever comes back.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::id::{MailboxId, RemoteMessageId};
use crate::mailbox::{MailboxName, MailboxRole};
use crate::message::{MessageFlags, ThreadingHeaders, UtcTimestamp};

/// A mailbox as reported by the remote server during discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMailbox {
    /// Decoded, full mailbox path (e.g. `INBOX/Work`).
    pub name: MailboxName,
    /// Role derived from server attributes (SPECIAL-USE) when
    /// available; `None` means the server gave no hint and the caller
    /// may fall back to name-based guessing.
    pub role: Option<MailboxRole>,
    /// Server hierarchy delimiter reported by `LIST` for this folder, or
    /// `None` when the protocol is flat (POP3) or the server omitted it.
    pub delimiter: Option<String>,
}

/// Snapshot of the remote folder structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMailboxState {
    /// All selectable mailboxes on the server.
    pub mailboxes: Vec<RemoteMailbox>,
}

/// Last locally known remote state of one mailbox, owned by storage
/// and passed into the adapter for incremental sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxSyncState {
    /// `UIDVALIDITY` recorded during the previous sync (IMAP).
    pub uid_validity: Option<u32>,
    /// Highest UID already ingested (IMAP).
    pub highest_uid: Option<u32>,
    /// `HIGHESTMODSEQ` recorded during the previous sync (IMAP
    /// CONDSTORE/QRESYNC). `None` when the server never reported one, which
    /// forces the adapter to fall back to the newest-UID flag-window scan and
    /// the present-set removal diff for this mailbox.
    #[serde(default)]
    pub highest_modseq: Option<u64>,
}

/// Instruction for one synchronization pass over one mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Local mailbox being synchronized.
    pub mailbox: MailboxId,
    /// Remote name of that mailbox.
    pub remote_name: MailboxName,
    /// Last known remote state; `Default` for a first sync.
    pub state: MailboxSyncState,
    /// Maximum number of envelopes to fetch in this pass.
    pub batch_size: u32,
    /// When set, the adapter must recompute removals from the authoritative
    /// present set even on a fast QRESYNC pass (the periodic safety net that
    /// backstops the VANISHED delta). The engine raises this on a documented
    /// cadence; adapters that always diff the present set (POP3, non-CONDSTORE
    /// IMAP) ignore it.
    #[serde(default)]
    pub full_reconcile: bool,
}

/// Envelope data for a message discovered on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteEnvelope {
    /// Identity on the server.
    pub remote_id: RemoteMessageId,
    /// Decoded subject, if present.
    pub subject: Option<String>,
    /// Senders.
    pub from: Vec<Address>,
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Date header, if present and parseable.
    pub date: Option<UtcTimestamp>,
    /// Server-side flags at fetch time.
    pub flags: MessageFlags,
    /// Size in bytes, if reported.
    pub size_bytes: Option<u64>,
    /// Threading identity, when the adapter scanned the full header block
    /// (POP3). `None` when only a server envelope was available (IMAP),
    /// leaving the storage layer to backfill it on first open.
    pub threading: Option<ThreadingHeaders>,
}

/// Result of one synchronization pass, to be persisted by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncOutcome {
    /// Current `UIDVALIDITY` of the mailbox (IMAP). A change versus
    /// the request's state invalidates every cached UID mapping.
    pub uid_validity: Option<u32>,
    /// Highest UID covered after this pass; feeds the next request.
    pub highest_uid: Option<u32>,
    /// `HIGHESTMODSEQ` observed at `SELECT` this pass (IMAP CONDSTORE),
    /// persisted so the next pass can request `CHANGEDSINCE` deltas. `None`
    /// when the server does not support mod-sequences for this mailbox
    /// (no CONDSTORE, or a `NOMODSEQ` select), keeping the legacy paths intact.
    #[serde(default)]
    pub highest_modseq: Option<u64>,
    /// Whether more messages remain beyond `batch_size`.
    pub has_more: bool,
    /// Newly discovered messages.
    pub new_envelopes: Vec<RemoteEnvelope>,
    /// Flag changes for already known messages.
    pub flag_updates: Vec<(RemoteMessageId, MessageFlags)>,
    /// Messages that vanished from the mailbox on the server.
    ///
    /// Only for adapters that can compute a *bounded* diff themselves
    /// (POP3 against its known-UIDL set). IMAP leaves this empty and
    /// reports [`SyncOutcome::present_remote_ids`] instead, since
    /// enumerating gaps in a sparse UID space would explode.
    pub removed: Vec<RemoteMessageId>,
    /// Authoritative, COMPLETE set of message ids currently present on
    /// the server (bounded by the real message count). When `Some`, the
    /// caller derives removals by deleting every stored id absent from
    /// this set — including ids ingested in the same pass, so the set
    /// must never be capped below the ids in `new_envelopes`.
    pub present_remote_ids: Option<Vec<RemoteMessageId>>,
}

/// Aggregate counters reported to the UI after a pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSummary {
    /// Messages newly seen on the server.
    pub new_messages: u64,
    /// Messages whose flags changed.
    pub updated_flags: u64,
    /// Messages no longer present on the server.
    pub removed_messages: u64,
}

impl SyncOutcome {
    /// Derives the UI-facing counters from this outcome.
    #[must_use]
    pub fn summary(&self) -> SyncSummary {
        SyncSummary {
            new_messages: self.new_envelopes.len() as u64,
            updated_flags: self.flag_updates.len() as u64,
            removed_messages: self.removed.len() as u64,
        }
    }
}

/// A raw, unparsed message as fetched from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    /// Identity of the message on the server.
    pub remote_id: RemoteMessageId,
    /// Complete RFC 5322 byte stream.
    pub bytes: Vec<u8>,
}

/// An attachment for an outgoing message.
///
/// Held in memory for the MVP; enforced size limits live in the
/// composer command layer. Streaming for large files is a later
/// optimization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingAttachment {
    /// Sanitized file name presented to recipients.
    pub filename: String,
    /// MIME type (e.g. `application/pdf`).
    pub mime_type: String,
    /// Raw content.
    pub bytes: Vec<u8>,
}

/// A message to be submitted via the outgoing provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Sender address.
    pub from: Address,
    /// Primary recipients; must not be empty at send time.
    pub to: Vec<Address>,
    /// Carbon-copy recipients.
    pub cc: Vec<Address>,
    /// Blind-carbon-copy recipients (never serialized into headers by
    /// the SMTP adapter).
    pub bcc: Vec<Address>,
    /// Optional reply-to address.
    pub reply_to: Option<Address>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body (always present; the plain view is mandatory).
    pub text_body: String,
    /// Optional HTML alternative, sent as `multipart/alternative`.
    pub html_body: Option<String>,
    /// File attachments.
    pub attachments: Vec<OutgoingAttachment>,
    /// `In-Reply-To`: the Message-ID (without angle brackets) of the message
    /// this one replies to. Written as a threading header by the SMTP adapter
    /// when present; `None` for a fresh message or a forward.
    pub in_reply_to: Option<String>,
    /// `References`: the ancestor Message-IDs (without angle brackets), oldest
    /// first, ending with the direct parent. Written as a threading header by
    /// the SMTP adapter when non-empty; empty for a fresh message or forward.
    pub references: Vec<String>,
    /// `Disposition-Notification-To`: when set, requests a message disposition
    /// notification (a read receipt, RFC 8098) be sent to this address. The
    /// SMTP adapter writes it as the header value verbatim; `None` requests no
    /// receipt.
    pub disposition_notification_to: Option<String>,
}

/// Acknowledgment for a submitted message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendResult {
    /// Redacted final server reply line (e.g. the `250` acknowledgment),
    /// kept for diagnostics.
    pub server_reply: Option<String>,
}

/// How an [`idle`](crate::IncomingMailProvider::idle) wait ended.
///
/// The push (IMAP IDLE, RFC 2177) supervisor drives one `idle` call per waiting
/// period and reacts to which of these it got back: [`Notified`](Self::Notified)
/// means the server pushed a mailbox change (trigger an immediate sync);
/// [`TimedOut`](Self::TimedOut) is the renewal cue (re-issue IDLE before the
/// server's ~29-minute inactivity limit); [`Interrupted`](Self::Interrupted)
/// means the caller's interrupt future fired (a clean shutdown/cancel, after
/// which the adapter has already sent `DONE`). In every case the adapter leaves
/// the session usable for the next call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdleOutcome {
    /// The server reported a change to the idled mailbox (new mail, an
    /// expunge, or a flag update). The caller should sync the mailbox now.
    Notified,
    /// The wait reached its renewal deadline without a server change. The
    /// caller should re-issue IDLE to keep the push connection alive.
    TimedOut,
    /// The caller's interrupt future fired (cancel/shutdown). `DONE` has been
    /// sent; the caller should stop idling.
    Interrupted,
}

/// Result of an `APPEND` of a message into a server mailbox.
///
/// `remote_id` is the server identity the appended message was assigned, when
/// the adapter could determine it. IMAP resolves it best-effort by matching the
/// message's `Message-ID` back to a UID in the target mailbox (async-imap does
/// not surface `APPENDUID`); a server that does not report the UID, or a
/// protocol without server identities, leaves it `None`. The caller uses it to
/// adopt the server copy onto its local record so a later sync does not
/// double-file the same message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendOutcome {
    /// The appended message's server identity, if the adapter resolved it.
    pub remote_id: Option<RemoteMessageId>,
}
