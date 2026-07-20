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
    /// Raw message bodies downloaded for offline availability this job.
    pub bodies_cached: u64,
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

/// The push (IMAP IDLE) connection state of one account, surfaced so the UI
/// can distinguish a live push connection from plain polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushState {
    /// A persistent IDLE connection is established and watching a mailbox; new
    /// mail triggers an immediate sync.
    Connected,
    /// No push connection: the server does not advertise IDLE, or push is
    /// disabled. The background poller keeps the mailbox fresh instead.
    Polling,
    /// The push connection dropped and is being re-established with backoff.
    Reconnecting,
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
    /// One write-back (`OpReplay`) drain finished, reporting how many queued
    /// operations were replayed to the server and how many were parked as
    /// failed. Emitted before the job's terminal [`Self::JobFinished`]; it
    /// carries the op counts the generic summary does not (`OpReplay` jobs
    /// ingest no messages, so their `JobFinished` summary is all zeros).
    OpsReplayed {
        /// Persisted job id.
        job_id: SyncJobId,
        /// Account whose queue was drained.
        account_id: AccountId,
        /// Operations successfully replayed to the server this drain.
        replayed: u64,
        /// Operations parked as failed (poison ops) this drain.
        failed: u64,
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
    /// The push (IMAP IDLE) connection state of an account changed. Emitted by
    /// the [`PushSupervisor`](crate::PushSupervisor); carries no job id.
    PushStateChanged {
        /// Account whose push state changed.
        account_id: AccountId,
        /// Mailbox the push connection watches when
        /// [`PushState::Connected`]; `None` for `Polling`/`Reconnecting`.
        mailbox_id: Option<MailboxId>,
        /// The new push connection state.
        state: PushState,
    },
}
