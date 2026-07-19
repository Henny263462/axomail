//! The offline operation queue backed by the `pending_ops` table.
//!
//! Each row is one server-bound operation (flag/keyword change, move, delete)
//! captured while offline or for asynchronous replay. Ops are ordered by `id`
//! per account and executed per message: the lowest-id un-completed op for a
//! message is its *head*, and [`PendingOpsRepo::claim_next_batch`] only ever
//! claims a **pending** head. An inflight or failed head therefore blocks
//! later ops for the *same* message, while ops for other messages keep
//! flowing — so a poison op wedges one message chain, never the whole account.
//!
//! Lifecycle: `pending -> inflight -> {deleted on success, pending on retry,
//! failed on park}`. A parked (`failed`) op is surfaced by
//! [`PendingOpsRepo::list_failed`] and un-parked by
//! [`PendingOpsRepo::requeue`]; [`PendingOpsRepo::requeue_stale`] recovers
//! ops a crash left `inflight`.

mod types;

use axomail_core::{AccountId, MailError, MessageId};
use sqlx::SqlitePool;

pub use types::{
    NewPendingOp, PendingOp, PendingOpCounts, PendingOpId, PendingOpKind, PendingOpState,
};

use crate::error_map::map_sqlx;

/// Columns selected when reading an op, in [`PendingOpRow`] order.
const OP_COLUMNS: &str = "id, account_id, message_id, kind, payload_json, state, \
    attempts, last_error, created_at_unix, updated_at_unix";

/// Repository over the `pending_ops` table.
#[derive(Debug)]
pub struct PendingOpsRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PendingOpsRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Enqueues a new `pending` op at the tail of its account's queue.
    ///
    /// The payload is validated as well-formed JSON first (the `address_json`
    /// precedent: opaque JSON text, validated in Rust).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the payload is not valid JSON,
    /// or [`MailError::Database`] on any query failure.
    pub async fn enqueue(&self, new: &NewPendingOp) -> Result<PendingOp, MailError> {
        validate_json(&new.payload_json)?;
        let row = sqlx::query_as::<_, PendingOpRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO pending_ops (account_id, message_id, kind, payload_json)
             VALUES (?, ?, ?, ?)
             RETURNING {OP_COLUMNS}"
        )))
        .bind(new.account_id.value())
        .bind(new.message_id.value())
        .bind(new.kind.as_str())
        .bind(&new.payload_json)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("enqueue_pending_op", error))?;
        row.into_op()
    }

    /// Atomically claims up to `limit` eligible ops for `account`, moving them
    /// `pending -> inflight`, and returns them in id order.
    ///
    /// An op is eligible only when it is the *head* of its message (no
    /// earlier op for the same message exists in any state) and is `pending`,
    /// so per-message ordering holds and an inflight/failed head blocks its
    /// own chain without stalling other messages. The single `UPDATE` is
    /// atomic and its `state = 'pending'` guard makes double-claiming
    /// impossible.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn claim_next_batch(
        &self,
        account: AccountId,
        limit: u32,
    ) -> Result<Vec<PendingOp>, MailError> {
        let rows = sqlx::query_as::<_, PendingOpRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE pending_ops
                SET state = 'inflight', updated_at_unix = unixepoch()
              WHERE id IN (
                  SELECT head.id FROM pending_ops head
                  WHERE head.account_id = ? AND head.state = 'pending'
                    AND NOT EXISTS (
                        SELECT 1 FROM pending_ops earlier
                        WHERE earlier.message_id = head.message_id
                          AND earlier.id < head.id
                    )
                  ORDER BY head.id
                  LIMIT ?
              )
              RETURNING {OP_COLUMNS}"
        )))
        .bind(account.value())
        .bind(i64::from(limit))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("claim_pending_ops", error))?;

        let mut ops = rows
            .into_iter()
            .map(PendingOpRow::into_op)
            .collect::<Result<Vec<_>, _>>()?;
        // RETURNING order is unspecified; restore the per-account id order.
        ops.sort_by_key(|op| op.id.value());
        Ok(ops)
    }

    /// Removes a completed op. Returns `true` if a row was deleted.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn complete(&self, id: PendingOpId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM pending_ops WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("complete_pending_op", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Records a transient failure and returns the op to `pending` for another
    /// attempt (attempts + 1). Only affects an `inflight` op.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn retry(&self, id: PendingOpId, error: &str) -> Result<bool, MailError> {
        self.transition(
            id,
            "inflight",
            "pending",
            true,
            Some(error),
            "retry_pending_op",
        )
        .await
    }

    /// Parks a poison op as `failed` (attempts + 1), blocking its message
    /// chain until it is requeued or dropped. Only affects an `inflight` op.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn fail(&self, id: PendingOpId, error: &str) -> Result<bool, MailError> {
        self.transition(
            id,
            "inflight",
            "failed",
            true,
            Some(error),
            "fail_pending_op",
        )
        .await
    }

    /// Un-parks a `failed` op back to `pending` for another attempt, without
    /// bumping the attempt count (an operator/scheduler action).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn requeue(&self, id: PendingOpId) -> Result<bool, MailError> {
        self.transition(id, "failed", "pending", false, None, "requeue_pending_op")
            .await
    }

    /// Recovers ops a crash left `inflight`, flipping them back to `pending`.
    /// Returns how many rows were requeued.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn requeue_stale(&self) -> Result<u64, MailError> {
        let result = sqlx::query(
            "UPDATE pending_ops SET state = 'pending', updated_at_unix = unixepoch()
             WHERE state = 'inflight'",
        )
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("requeue_stale_pending_ops", error))?;
        Ok(result.rows_affected())
    }

    /// Returns the ids of accounts that currently have at least one *claimable*
    /// pending op.
    ///
    /// "Claimable" mirrors [`Self::claim_next_batch`]'s eligibility exactly: a
    /// `pending` op that is the head of its message (no earlier op in any state).
    /// An account whose only remaining ops are parked (`failed`) — or `pending`
    /// but blocked behind a parked head — is **excluded**, so a caller can
    /// enqueue a drain without spinning on work no drain can make progress on.
    ///
    /// Used by the engine's startup recovery to drain ops a crash left behind
    /// with no live replay job.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn accounts_with_claimable_ops(&self) -> Result<Vec<AccountId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>(
            "SELECT DISTINCT head.account_id FROM pending_ops head
              WHERE head.state = 'pending'
                AND NOT EXISTS (
                    SELECT 1 FROM pending_ops earlier
                    WHERE earlier.message_id = head.message_id
                      AND earlier.id < head.id
                )",
        )
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("accounts_with_claimable_ops", error))?;
        Ok(ids.into_iter().map(AccountId::new).collect())
    }

    /// Whether `account` has at least one *claimable* pending op (same
    /// eligibility as [`Self::accounts_with_claimable_ops`]).
    ///
    /// Used to decide, after a replay drain ends, whether a fresh drain is worth
    /// enqueuing — without looping forever on ops blocked behind a parked head.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn account_has_claimable_ops(&self, account: AccountId) -> Result<bool, MailError> {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(
                 SELECT 1 FROM pending_ops head
                  WHERE head.account_id = ? AND head.state = 'pending'
                    AND NOT EXISTS (
                        SELECT 1 FROM pending_ops earlier
                        WHERE earlier.message_id = head.message_id
                          AND earlier.id < head.id
                    )
             )",
        )
        .bind(account.value())
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("account_has_claimable_ops", error))?;
        Ok(exists != 0)
    }

    /// Lists an account's parked (`failed`) ops in id order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a decode error.
    pub async fn list_failed(&self, account: AccountId) -> Result<Vec<PendingOp>, MailError> {
        let rows = sqlx::query_as::<_, PendingOpRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OP_COLUMNS} FROM pending_ops
             WHERE account_id = ? AND state = 'failed' ORDER BY id"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_failed_pending_ops", error))?;
        rows.into_iter().map(PendingOpRow::into_op).collect()
    }

    /// Returns per-state counts for an account (for UI/activity surfaces).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn counts(&self, account: AccountId) -> Result<PendingOpCounts, MailError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT state, COUNT(*) FROM pending_ops WHERE account_id = ? GROUP BY state",
        )
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("count_pending_ops", error))?;

        let mut counts = PendingOpCounts::default();
        for (state, count) in rows {
            match PendingOpState::parse(&state)? {
                PendingOpState::Pending => counts.pending = count,
                PendingOpState::Inflight => counts.inflight = count,
                PendingOpState::Failed => counts.failed = count,
            }
        }
        Ok(counts)
    }

    /// Shared state transition: moves `id` from `from` to `to`, optionally
    /// bumping attempts and recording an error. Returns `true` if a row moved.
    async fn transition(
        &self,
        id: PendingOpId,
        from: &'static str,
        to: &'static str,
        bump_attempts: bool,
        error: Option<&str>,
        op_name: &'static str,
    ) -> Result<bool, MailError> {
        let attempts = if bump_attempts {
            "attempts = attempts + 1, "
        } else {
            ""
        };
        let result = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE pending_ops
                SET state = ?, {attempts}last_error = ?, updated_at_unix = unixepoch()
              WHERE id = ? AND state = ?"
        )))
        .bind(to)
        .bind(error)
        .bind(id.value())
        .bind(from)
        .execute(self.pool)
        .await
        .map_err(|sqlx_error| map_sqlx(op_name, sqlx_error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Validates that `payload` is well-formed JSON before it reaches storage.
fn validate_json(payload: &str) -> Result<(), MailError> {
    serde_json::from_str::<serde_json::Value>(payload)
        .map(|_| ())
        .map_err(|_| MailError::Configuration {
            message: "pending op payload is not valid JSON".to_owned(),
        })
}

/// Raw `pending_ops` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct PendingOpRow {
    id: i64,
    account_id: i64,
    message_id: i64,
    kind: String,
    payload_json: String,
    state: String,
    attempts: i64,
    last_error: Option<String>,
    created_at_unix: i64,
    updated_at_unix: i64,
}

impl PendingOpRow {
    fn into_op(self) -> Result<PendingOp, MailError> {
        Ok(PendingOp {
            id: PendingOpId::new(self.id),
            account_id: AccountId::new(self.account_id),
            message_id: MessageId::new(self.message_id),
            kind: PendingOpKind::parse(&self.kind)?,
            payload_json: self.payload_json,
            state: PendingOpState::parse(&self.state)?,
            attempts: self.attempts,
            last_error: self.last_error,
            created_at_unix: self.created_at_unix,
            updated_at_unix: self.updated_at_unix,
        })
    }
}
