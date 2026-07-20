//! Per-mailbox message-retention policy and the query that selects the
//! over-retention messages a maintenance pass acts on (track 5.5).
//!
//! A mailbox's [`RetentionPolicy`] bounds how much mail is kept: by age, by
//! count, or not at all. The command layer's `apply_folder_retention` reads the
//! policy, asks this repo for the over-retention message ids, and disposes of
//! them through one of two safe paths (move to Trash via the guarded write-back
//! queue, or drop the local cached body) — it never hard-deletes server mail.
//!
//! Starred (`\Flagged`) messages are always excluded from the candidate set, so
//! retention can never silently discard a message the user marked important.

use axomail_core::{MailError, MailboxId, MessageId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Seconds in a day, for the `ByAge` window cutoff.
const SECONDS_PER_DAY: i64 = 86_400;

/// Which messages a mailbox keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionMode {
    /// Retention is off; keep everything (the maintenance pass is a no-op).
    KeepAll,
    /// Keep only messages newer than the policy's day window.
    ByAge,
    /// Keep only the newest N messages.
    ByCount,
}

impl RetentionMode {
    /// Serialized form stored in the `retention_mode` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeepAll => "keep_all",
            Self::ByAge => "by_age",
            Self::ByCount => "by_count",
        }
    }

    /// Parses a stored mode string.
    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "keep_all" => Ok(Self::KeepAll),
            "by_age" => Ok(Self::ByAge),
            "by_count" => Ok(Self::ByCount),
            _ => Err(MailError::Database {
                message: "read_retention_policy: unknown retention mode".to_owned(),
            }),
        }
    }
}

/// How the maintenance pass disposes of an over-retention message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    /// Move it to the account's Trash folder (guarded write-back path).
    Trash,
    /// Drop its local cached body only; the envelope stays.
    PurgeBody,
}

impl RetentionAction {
    /// Serialized form stored in the `retention_action` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trash => "trash",
            Self::PurgeBody => "purge_body",
        }
    }

    /// Parses a stored action string.
    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "trash" => Ok(Self::Trash),
            "purge_body" => Ok(Self::PurgeBody),
            _ => Err(MailError::Database {
                message: "read_retention_policy: unknown retention action".to_owned(),
            }),
        }
    }
}

/// A mailbox's resolved retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Which messages the mailbox keeps.
    pub mode: RetentionMode,
    /// Age boundary (days) honoured in [`RetentionMode::ByAge`].
    pub days: u32,
    /// Keep-count honoured in [`RetentionMode::ByCount`].
    pub max_count: u32,
    /// How over-retention messages are disposed of.
    pub action: RetentionAction,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            mode: RetentionMode::KeepAll,
            days: 30,
            max_count: 1000,
            action: RetentionAction::Trash,
        }
    }
}

/// Repository over the retention-policy columns on `mailboxes`.
#[derive(Debug)]
pub struct RetentionRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RetentionRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Reads a mailbox's retention policy. A mailbox absent from the table
    /// resolves to the schema default (`keep_all`) so callers never
    /// special-case a missing row.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn get(&self, mailbox: MailboxId) -> Result<RetentionPolicy, MailError> {
        let row: Option<(String, i64, i64, String)> = sqlx::query_as(
            "SELECT retention_mode, retention_days, retention_max_count, retention_action
             FROM mailboxes WHERE id = ?",
        )
        .bind(mailbox.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_retention_policy", error))?;
        match row {
            Some((mode, days, max_count, action)) => Ok(RetentionPolicy {
                mode: RetentionMode::parse(&mode)?,
                days: u32::try_from(days).unwrap_or(30).max(1),
                max_count: u32::try_from(max_count).unwrap_or(1000).max(1),
                action: RetentionAction::parse(&action)?,
            }),
            None => Ok(RetentionPolicy::default()),
        }
    }

    /// Sets a mailbox's retention policy. Returns whether the mailbox exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set(
        &self,
        mailbox: MailboxId,
        policy: RetentionPolicy,
    ) -> Result<bool, MailError> {
        let result = sqlx::query(
            "UPDATE mailboxes
                SET retention_mode = ?, retention_days = ?,
                    retention_max_count = ?, retention_action = ?
              WHERE id = ?",
        )
        .bind(policy.mode.as_str())
        .bind(i64::from(policy.days.max(1)))
        .bind(i64::from(policy.max_count.max(1)))
        .bind(policy.action.as_str())
        .bind(mailbox.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("set_retention_policy", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Selects the over-retention message ids in `mailbox` under its stored
    /// policy, oldest first. Never includes a starred (`\Flagged`) message.
    ///
    /// `cached_only` restricts the result to messages that currently hold a
    /// locally-cached raw body (`has_raw = 1`) — used by the `purge_body`
    /// disposition, which has nothing to do for a message with no cached body.
    /// `now_unix` anchors the [`RetentionMode::ByAge`] window; pass the current
    /// wall clock.
    ///
    /// `KeepAll` always yields an empty set.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn over_retention(
        &self,
        mailbox: MailboxId,
        now_unix: i64,
        cached_only: bool,
    ) -> Result<Vec<MessageId>, MailError> {
        let policy = self.get(mailbox).await?;
        // A compile-time-constant fragment; never interpolates user input.
        let cached_clause = if cached_only { "AND has_raw = 1" } else { "" };
        let ids: Vec<i64> = match policy.mode {
            RetentionMode::KeepAll => return Ok(Vec::new()),
            RetentionMode::ByAge => {
                let cutoff = now_unix - i64::from(policy.days.max(1)) * SECONDS_PER_DAY;
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT id FROM messages
                      WHERE mailbox_id = ? AND flag_flagged = 0 {cached_clause}
                        AND date_unix IS NOT NULL AND date_unix < ?
                      ORDER BY date_unix ASC, id ASC"
                )))
                .bind(mailbox.value())
                .bind(cutoff)
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("over_retention_by_age", error))?
            }
            RetentionMode::ByCount => {
                // The kept set is the newest N of *all* messages (a starred
                // message occupies a slot and is kept anyway); candidates are
                // the rest, minus starred, oldest first.
                let keep = i64::from(policy.max_count.max(1));
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT id FROM messages
                      WHERE mailbox_id = ? AND flag_flagged = 0 {cached_clause}
                        AND id NOT IN (
                            SELECT id FROM messages
                             WHERE mailbox_id = ?
                             ORDER BY date_unix DESC, id DESC
                             LIMIT ?
                        )
                      ORDER BY date_unix ASC, id ASC"
                )))
                .bind(mailbox.value())
                .bind(mailbox.value())
                .bind(keep)
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("over_retention_by_count", error))?
            }
        };
        Ok(ids.into_iter().map(MessageId::new).collect())
    }
}
