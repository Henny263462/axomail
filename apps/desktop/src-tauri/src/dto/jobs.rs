//! Activity-manager DTOs: a background sync job with its display context
//! (account and mailbox names) resolved so the frontend can render the
//! activity list without a second round trip.

use std::collections::HashMap;

use axomail_core::MailError;
use axomail_storage::{JobKind, JobState, SyncJob};
use serde::Serialize;

use super::checked_id;

/// A background sync job as shown in the activity manager.
///
/// Ids are validated for the JavaScript boundary. `account_label` and
/// `mailbox_label` carry the human-readable names resolved from storage; a
/// label is `None` when its owning row no longer exists (for example, the
/// account was deleted after the job finished).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncJobDto {
    /// Local job identifier.
    pub id: i64,
    /// Kind of work, camelCase (`mailboxSync`, `discover`, `fetch`, `send`).
    pub kind: &'static str,
    /// Lifecycle state (`pending`, `running`, `succeeded`, `failed`,
    /// `cancelled`).
    pub state: &'static str,
    /// Owning account id, when the job is account-scoped.
    pub account_id: Option<i64>,
    /// Owning account's display name, when it still exists.
    pub account_label: Option<String>,
    /// Target mailbox id, when the job is mailbox-scoped.
    pub mailbox_id: Option<i64>,
    /// Target mailbox's name, when it still exists.
    pub mailbox_label: Option<String>,
    /// Redacted failure reason, when the job failed.
    pub error: Option<String>,
    /// When the job was claimed (Unix seconds), if ever.
    pub started_at_unix: Option<i64>,
    /// When the job reached a terminal state (Unix seconds), if ever.
    pub finished_at_unix: Option<i64>,
}

impl SyncJobDto {
    /// Builds a DTO from a job row, resolving labels from the supplied maps
    /// and validating every id for the JavaScript boundary.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if any id is out of the safe
    /// integer range.
    pub fn from_job(
        job: &SyncJob,
        account_labels: &HashMap<i64, String>,
        mailbox_labels: &HashMap<i64, String>,
    ) -> Result<Self, MailError> {
        let account_id = match job.account_id {
            Some(id) => Some(checked_id(id.value())?),
            None => None,
        };
        let mailbox_id = match job.mailbox_id {
            Some(id) => Some(checked_id(id.value())?),
            None => None,
        };
        Ok(Self {
            id: checked_id(job.id.value())?,
            kind: kind_str(job.kind),
            state: state_str(job.state),
            account_id,
            account_label: account_id.and_then(|id| account_labels.get(&id).cloned()),
            mailbox_id,
            mailbox_label: mailbox_id.and_then(|id| mailbox_labels.get(&id).cloned()),
            error: job.error.clone(),
            started_at_unix: job.started_at_unix,
            finished_at_unix: job.finished_at_unix,
        })
    }
}

/// Stable camelCase name for a job kind (mirrors the wire contract).
fn kind_str(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Discover => "discover",
        JobKind::MailboxSync => "mailboxSync",
        JobKind::Fetch => "fetch",
        JobKind::Send => "send",
    }
}

/// Stable name for a job state (single lowercase words need no re-casing).
fn state_str(state: JobState) -> &'static str {
    match state {
        JobState::Pending => "pending",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}
