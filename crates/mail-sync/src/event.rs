//! Progress events broadcast to observers (notably the Tauri layer).
//!
//! Events are `serde`-serializable so the app can forward them to the
//! frontend unchanged. They are delivered over a `tokio::sync::broadcast`
//! channel; a slow observer may miss events under load but never blocks
//! the engine.

use axomail_core::{AccountId, ErrorClass, MailboxId};
use axomail_storage::{IngestSummary, SyncJobId};
use serde::{Deserialize, Serialize};

/// Serializable mirror of [`axomail_core::ErrorClass`] for events.
///
/// The core type is intentionally not `serde`-aware; this mirror keeps
/// the wire contract owned by the sync layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncErrorClass {
    /// Retrying later may succeed.
    Temporary,
    /// Retrying will not help without a change.
    Permanent,
    /// The user must fix account or app configuration.
    UserConfiguration,
}

impl From<ErrorClass> for SyncErrorClass {
    fn from(class: ErrorClass) -> Self {
        match class {
            ErrorClass::Temporary => Self::Temporary,
            ErrorClass::Permanent => Self::Permanent,
            ErrorClass::UserConfiguration => Self::UserConfiguration,
        }
    }
}

/// Aggregate progress of one completed job, summed over its passes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncJobSummary {
    /// Total new messages ingested.
    pub new_messages: u64,
    /// Total messages whose flags changed.
    pub updated_flags: u64,
    /// Total messages removed.
    pub removed_messages: u64,
    /// Number of sync passes applied.
    pub batches: u64,
}

impl SyncJobSummary {
    /// Folds one pass's ingest result into the running summary.
    pub(crate) fn absorb(&mut self, ingest: &IngestSummary) {
        self.new_messages += ingest.inserted;
        self.updated_flags += ingest.flags_updated;
        self.removed_messages += ingest.removed;
        self.batches += 1;
    }
}

/// A progress event emitted during a job's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncEvent {
    /// A new job was accepted and persisted as pending.
    JobQueued {
        /// Persisted job id.
        job_id: SyncJobId,
        /// Owning account.
        account_id: AccountId,
        /// Target mailbox, when the job is mailbox-scoped. `None` for
        /// account-scoped kinds (`Discover`, `OpReplay`).
        mailbox_id: Option<MailboxId>,
    },
    /// A job was claimed and started executing.
    JobStarted {
        /// Persisted job id.
        job_id: SyncJobId,
    },
    /// One sync pass was ingested.
    BatchIngested {
        /// Persisted job id.
        job_id: SyncJobId,
        /// Mailbox that received the batch.
        mailbox_id: MailboxId,
        /// New messages ingested in this pass.
        new_messages: u64,
    },
    /// A job completed successfully.
    JobFinished {
        /// Persisted job id.
        job_id: SyncJobId,
        /// Aggregate progress.
        summary: SyncJobSummary,
    },
    /// A job was cancelled cooperatively before completion.
    ///
    /// This is a distinct terminal state from [`Self::JobFailed`]: a
    /// cancelled job is a deliberate user action, not an error, so the
    /// frontend must not surface it as a failure (see the activity manager
    /// and `dto/sync_event.rs`).
    JobCancelled {
        /// Persisted job id.
        job_id: SyncJobId,
    },
    /// A job ended with an error (never cancellation — see
    /// [`Self::JobCancelled`]).
    JobFailed {
        /// Persisted job id.
        job_id: SyncJobId,
        /// Classification of the terminating error.
        class: SyncErrorClass,
        /// Redacted, user-safe reason.
        message: String,
    },
}
