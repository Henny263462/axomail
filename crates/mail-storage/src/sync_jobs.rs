//! Persistent background-job tracking backed by the `sync_jobs` table.
//!
//! A job moves `pending -> running -> {succeeded, failed, cancelled}`.
//! The engine claims a pending job (an atomic `pending -> running`
//! transition), works it, then records a terminal state with an optional
//! redacted error and a finish timestamp. [`SyncJobRepo::requeue_stale`]
//! flips any `running` rows left behind by a crash back to `pending` so
//! they resume on the next startup.

use axomail_core::{AccountId, MailError, MailboxId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Identifier of a persisted [`SyncJob`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyncJobId(i64);

impl SyncJobId {
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

impl std::fmt::Display for SyncJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of work a job represents, matching the schema's `kind` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Discover the remote folder structure of an account.
    Discover,
    /// Incrementally synchronize one mailbox.
    MailboxSync,
    /// Fetch one full message body.
    Fetch,
    /// Submit one outgoing message.
    Send,
}

impl JobKind {
    /// Serialized form stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::MailboxSync => "mailbox_sync",
            Self::Fetch => "fetch",
            Self::Send => "send",
        }
    }

    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "discover" => Ok(Self::Discover),
            "mailbox_sync" => Ok(Self::MailboxSync),
            "fetch" => Ok(Self::Fetch),
            "send" => Ok(Self::Send),
            _ => Err(MailError::Database {
                message: "read_sync_job: unknown job kind".to_owned(),
            }),
        }
    }
}

/// Lifecycle state of a job, matching the schema's `state` domain.
///
/// `Succeeded` is the successful terminal state (the schema keeps
/// `succeeded` rather than `done`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Waiting to be claimed.
    Pending,
    /// Claimed and executing.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Finished with an error.
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl JobState {
    /// Serialized form stored in the `state` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(MailError::Database {
                message: "read_sync_job: unknown job state".to_owned(),
            }),
        }
    }
}

/// A job to create, before an identifier has been assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSyncJob {
    /// Kind of work.
    pub kind: JobKind,
    /// Owning account, if the job is account-scoped.
    pub account_id: Option<AccountId>,
    /// Target mailbox, if the job is mailbox-scoped.
    pub mailbox_id: Option<MailboxId>,
}

/// A persisted job row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncJob {
    /// Row identifier.
    pub id: SyncJobId,
    /// Kind of work.
    pub kind: JobKind,
    /// Current lifecycle state.
    pub state: JobState,
    /// Owning account, if any.
    pub account_id: Option<AccountId>,
    /// Target mailbox, if any.
    pub mailbox_id: Option<MailboxId>,
    /// Redacted failure reason, if the job failed.
    pub error: Option<String>,
    /// When the job was claimed (Unix seconds), if ever.
    pub started_at_unix: Option<i64>,
    /// When the job reached a terminal state (Unix seconds), if ever.
    pub finished_at_unix: Option<i64>,
}

/// Columns selected when reading a job, in [`SyncJobRow`] order.
const JOB_COLUMNS: &str = "id, kind, state, account_id, mailbox_id, \
    error, started_at_unix, finished_at_unix";

/// Repository over the `sync_jobs` table.
#[derive(Debug)]
pub struct SyncJobRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SyncJobRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates a `pending` job, but only if no equivalent job of the
    /// same kind and mailbox is already `pending` or `running`.
    ///
    /// The check and insert are one atomic statement so concurrent
    /// enqueues cannot both create a duplicate. Returns `None` when an
    /// equivalent active job already exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create_if_absent(&self, new: &NewSyncJob) -> Result<Option<SyncJob>, MailError> {
        let account_id = new.account_id.map(AccountId::value);
        let mailbox_id = new.mailbox_id.map(MailboxId::value);

        let row: Option<SyncJobRow> =
            sqlx::query_as::<_, SyncJobRow>(sqlx::AssertSqlSafe(format!(
                "INSERT INTO sync_jobs (kind, account_id, mailbox_id, state)
             SELECT ?, ?, ?, 'pending'
             WHERE NOT EXISTS (
                 SELECT 1 FROM sync_jobs
                 WHERE kind = ? AND mailbox_id IS ? AND state IN ('pending', 'running')
             )
             RETURNING {JOB_COLUMNS}"
            )))
            .bind(new.kind.as_str())
            .bind(account_id)
            .bind(mailbox_id)
            .bind(new.kind.as_str())
            .bind(mailbox_id)
            .fetch_optional(self.pool)
            .await
            .map_err(|error| map_sqlx("create_sync_job", error))?;

        row.map(SyncJobRow::into_job).transpose()
    }

    /// Atomically claims a `pending` job, moving it to `running` and
    /// stamping the start time. Returns `None` if the job is absent or
    /// no longer pending (already claimed elsewhere).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn claim(&self, id: SyncJobId) -> Result<Option<SyncJob>, MailError> {
        let row: Option<SyncJobRow> =
            sqlx::query_as::<_, SyncJobRow>(sqlx::AssertSqlSafe(format!(
                "UPDATE sync_jobs SET state = 'running', started_at_unix = unixepoch()
             WHERE id = ? AND state = 'pending'
             RETURNING {JOB_COLUMNS}"
            )))
            .bind(id.value())
            .fetch_optional(self.pool)
            .await
            .map_err(|error| map_sqlx("claim_sync_job", error))?;

        row.map(SyncJobRow::into_job).transpose()
    }

    /// Records a terminal `state` for a job together with an optional
    /// redacted error message and the finish time.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn finish(
        &self,
        id: SyncJobId,
        state: JobState,
        error: Option<&str>,
    ) -> Result<(), MailError> {
        sqlx::query(
            "UPDATE sync_jobs SET state = ?, error = ?, finished_at_unix = unixepoch()
             WHERE id = ?",
        )
        .bind(state.as_str())
        .bind(error)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("finish_sync_job", error))?;
        Ok(())
    }

    /// Requeues jobs left `running` by a crash, flipping them back to
    /// `pending` and clearing their start time. Returns how many rows
    /// were requeued.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn requeue_stale(&self) -> Result<u64, MailError> {
        let result =
            sqlx::query("UPDATE sync_jobs SET state = 'pending', started_at_unix = NULL WHERE state = 'running'")
                .execute(self.pool)
                .await
                .map_err(|error| map_sqlx("requeue_stale_sync_jobs", error))?;
        Ok(result.rows_affected())
    }

    /// Lists jobs in a given state, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a
    /// stored row cannot be decoded.
    pub async fn list_by_state(&self, state: JobState) -> Result<Vec<SyncJob>, MailError> {
        let rows = sqlx::query_as::<_, SyncJobRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_COLUMNS} FROM sync_jobs WHERE state = ? ORDER BY id"
        )))
        .bind(state.as_str())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_sync_jobs", error))?;

        rows.into_iter().map(SyncJobRow::into_job).collect()
    }

    /// Fetches a single job by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a
    /// stored row cannot be decoded.
    pub async fn get(&self, id: SyncJobId) -> Result<Option<SyncJob>, MailError> {
        let row = sqlx::query_as::<_, SyncJobRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_COLUMNS} FROM sync_jobs WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_sync_job", error))?;

        row.map(SyncJobRow::into_job).transpose()
    }
}

/// Raw `sync_jobs` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct SyncJobRow {
    id: i64,
    kind: String,
    state: String,
    account_id: Option<i64>,
    mailbox_id: Option<i64>,
    error: Option<String>,
    started_at_unix: Option<i64>,
    finished_at_unix: Option<i64>,
}

impl SyncJobRow {
    fn into_job(self) -> Result<SyncJob, MailError> {
        Ok(SyncJob {
            id: SyncJobId::new(self.id),
            kind: JobKind::parse(&self.kind)?,
            state: JobState::parse(&self.state)?,
            account_id: self.account_id.map(AccountId::new),
            mailbox_id: self.mailbox_id.map(MailboxId::new),
            error: self.error,
            started_at_unix: self.started_at_unix,
            finished_at_unix: self.finished_at_unix,
        })
    }
}
