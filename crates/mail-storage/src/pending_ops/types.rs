//! Value types for the offline operation queue: identifiers, the kind and
//! state enums (CHECK-mirrored), and the new/persisted row structs.

use axomail_core::{AccountId, MailError, MessageId};
use serde::{Deserialize, Serialize};

/// Identifier of a persisted [`PendingOp`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PendingOpId(i64);

impl PendingOpId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for PendingOpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of offline operation, matching the schema's `kind` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingOpKind {
    /// Add system flags (`STORE +FLAGS`).
    AddFlags,
    /// Remove system flags (`STORE -FLAGS`).
    RemoveFlags,
    /// Add an IMAP keyword.
    AddKeyword,
    /// Remove an IMAP keyword.
    RemoveKeyword,
    /// Move the message to another mailbox.
    Move,
    /// Delete the message (to Trash / expunge, per policy).
    Delete,
}

impl PendingOpKind {
    /// Serialized form stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddFlags => "add_flags",
            Self::RemoveFlags => "remove_flags",
            Self::AddKeyword => "add_keyword",
            Self::RemoveKeyword => "remove_keyword",
            Self::Move => "move",
            Self::Delete => "delete",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "add_flags" => Ok(Self::AddFlags),
            "remove_flags" => Ok(Self::RemoveFlags),
            "add_keyword" => Ok(Self::AddKeyword),
            "remove_keyword" => Ok(Self::RemoveKeyword),
            "move" => Ok(Self::Move),
            "delete" => Ok(Self::Delete),
            _ => Err(MailError::Database {
                message: "read_pending_op: unknown op kind".to_owned(),
            }),
        }
    }
}

/// Lifecycle state of an offline operation, matching the schema's `state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingOpState {
    /// Waiting to be claimed.
    Pending,
    /// Claimed and being replayed.
    Inflight,
    /// Parked after a failure; blocks its message chain until requeued.
    Failed,
}

impl PendingOpState {
    pub(super) fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "pending" => Ok(Self::Pending),
            "inflight" => Ok(Self::Inflight),
            "failed" => Ok(Self::Failed),
            _ => Err(MailError::Database {
                message: "read_pending_op: unknown op state".to_owned(),
            }),
        }
    }
}

/// An operation to enqueue, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPendingOp {
    /// Owning account (queue is per-account).
    pub account_id: AccountId,
    /// Message the operation targets.
    pub message_id: MessageId,
    /// Kind of operation.
    pub kind: PendingOpKind,
    /// Opaque JSON payload (validated as well-formed JSON on enqueue).
    pub payload_json: String,
}

/// A persisted offline operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOp {
    /// Row identifier; also the per-account ordering key.
    pub id: PendingOpId,
    /// Owning account.
    pub account_id: AccountId,
    /// Target message.
    pub message_id: MessageId,
    /// Kind of operation.
    pub kind: PendingOpKind,
    /// Opaque JSON payload.
    pub payload_json: String,
    /// Current lifecycle state.
    pub state: PendingOpState,
    /// Number of replay attempts so far.
    pub attempts: i64,
    /// Redacted last failure reason, if any.
    pub last_error: Option<String>,
    /// When the op was enqueued (Unix seconds).
    pub created_at_unix: i64,
    /// When the op was last updated (Unix seconds).
    pub updated_at_unix: i64,
}

/// Per-account counts by state, for the activity/UI surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOpCounts {
    /// Ops waiting to be claimed.
    pub pending: i64,
    /// Ops currently being replayed.
    pub inflight: i64,
    /// Ops parked after a failure.
    pub failed: i64,
}
