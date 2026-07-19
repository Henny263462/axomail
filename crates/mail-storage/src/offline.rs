//! Per-mailbox offline policy and the queries that drive sync-time body
//! download (track 2.3).
//!
//! A mailbox's [`OfflineMode`] decides which of its messages get their full
//! raw body pre-downloaded and indexed during sync. The sync worker asks for a
//! bounded [`DownloadPlan`] per pass (so a large backlog progresses
//! incrementally, honouring the engine's commit-then-continue shape), caches
//! each body, and calls [`OfflinePolicyRepo::mark_downloaded`] to flip
//! `has_raw` and index the body atomically. The reading pane's lazy fetch stays
//! the fallback for `'none'`.

use axomail_core::{MailError, MailboxId, MessageId, RemoteMessageId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;
use crate::message_rows::decode_remote_id;

/// Seconds in a day, for the `'recent'` window cutoff.
const SECONDS_PER_DAY: i64 = 86_400;

/// How much of a mailbox is kept available offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineMode {
    /// Download nothing eagerly; bodies are lazy-fetched on first open.
    None,
    /// Download bodies newer than the mailbox's recent-day window.
    Recent,
    /// Download every message's body.
    All,
}

impl OfflineMode {
    /// Serialized form stored in the `offline_mode` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Recent => "recent",
            Self::All => "all",
        }
    }

    /// Parses a stored mode string.
    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "none" => Ok(Self::None),
            "recent" => Ok(Self::Recent),
            "all" => Ok(Self::All),
            _ => Err(MailError::Database {
                message: "read_offline_policy: unknown offline mode".to_owned(),
            }),
        }
    }
}

/// A mailbox's resolved offline policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePolicy {
    /// Which messages to keep available offline.
    pub mode: OfflineMode,
    /// Age boundary (in days) for [`OfflineMode::Recent`]; ignored otherwise.
    pub recent_days: u32,
}

/// One message whose raw body should be downloaded and cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCandidate {
    /// Local message id (raw file name and `has_raw` target).
    pub message_id: MessageId,
    /// Remote identity to fetch the body by.
    pub remote_id: RemoteMessageId,
}

/// A bounded unit of offline-download work for one sync pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Messages to fetch this pass (already bounded by the caller's limit).
    pub candidates: Vec<DownloadCandidate>,
    /// Whether a "download now" override forced a full download (as if
    /// [`OfflineMode::All`]) regardless of the standing policy. When set and
    /// the batch was not full, the worker clears the override.
    pub force_full: bool,
}

/// Repository over the offline-policy columns on `mailboxes` and the raw-body
/// bookkeeping on `messages`.
#[derive(Debug)]
pub struct OfflinePolicyRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> OfflinePolicyRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Reads a mailbox's offline policy. A mailbox absent from the table
    /// resolves to the schema default (`recent`, 30 days) so callers never
    /// special-case a missing row.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn get(&self, mailbox: MailboxId) -> Result<OfflinePolicy, MailError> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT offline_mode, offline_recent_days FROM mailboxes WHERE id = ?")
                .bind(mailbox.value())
                .fetch_optional(self.pool)
                .await
                .map_err(|error| map_sqlx("get_offline_policy", error))?;
        match row {
            Some((mode, days)) => Ok(OfflinePolicy {
                mode: OfflineMode::parse(&mode)?,
                recent_days: u32::try_from(days).unwrap_or(30).max(1),
            }),
            None => Ok(OfflinePolicy {
                mode: OfflineMode::Recent,
                recent_days: 30,
            }),
        }
    }

    /// Sets a mailbox's offline policy. Returns whether the mailbox exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set(
        &self,
        mailbox: MailboxId,
        mode: OfflineMode,
        recent_days: u32,
    ) -> Result<bool, MailError> {
        let days = i64::from(recent_days.max(1));
        let result = sqlx::query(
            "UPDATE mailboxes SET offline_mode = ?, offline_recent_days = ? WHERE id = ?",
        )
        .bind(mode.as_str())
        .bind(days)
        .bind(mailbox.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("set_offline_policy", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Sets (or clears) the transient "download now" full-download override.
    /// Returns whether the mailbox exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_pending_full_download(
        &self,
        mailbox: MailboxId,
        pending: bool,
    ) -> Result<bool, MailError> {
        let result = sqlx::query("UPDATE mailboxes SET pending_full_download = ? WHERE id = ?")
            .bind(i64::from(pending))
            .bind(mailbox.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("set_pending_full_download", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Builds a bounded download plan for one sync pass over `mailbox`.
    ///
    /// Effective mode is `All` when the "download now" override is set,
    /// otherwise the stored policy. Candidates are messages with a remote
    /// identity and no cached raw body (`has_raw = 0`) covered by that mode,
    /// newest first and capped at `limit`. `now_unix` anchors the `recent`
    /// window; pass the current wall clock.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a row that
    /// cannot be decoded.
    pub async fn download_plan(
        &self,
        mailbox: MailboxId,
        now_unix: i64,
        limit: u32,
    ) -> Result<DownloadPlan, MailError> {
        let (mode_str, recent_days, force_full): (String, i64, i64) = sqlx::query_as(
            "SELECT offline_mode, offline_recent_days, pending_full_download
             FROM mailboxes WHERE id = ?",
        )
        .bind(mailbox.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("download_plan_policy", error))?
        .unwrap_or_else(|| ("recent".to_owned(), 30, 0));

        let force_full = force_full != 0;
        let mode = if force_full {
            OfflineMode::All
        } else {
            OfflineMode::parse(&mode_str)?
        };
        let candidates = self
            .candidates(mailbox, mode, recent_days, now_unix, limit)
            .await?;
        Ok(DownloadPlan {
            candidates,
            force_full,
        })
    }

    /// Runs the candidate query for a resolved mode.
    async fn candidates(
        &self,
        mailbox: MailboxId,
        mode: OfflineMode,
        recent_days: i64,
        now_unix: i64,
        limit: u32,
    ) -> Result<Vec<DownloadCandidate>, MailError> {
        let limit = i64::from(limit.max(1));
        // `NULL` dates sort last under `DESC`, so 'all' downloads dated
        // messages before undated ones. 'recent' excludes NULL dates because
        // `date_unix >= cutoff` is never true for them.
        let base = "SELECT id, remote_kind, remote_uid, remote_uid_validity, remote_uidl \
             FROM messages \
             WHERE mailbox_id = ? AND has_raw = 0 AND remote_kind IS NOT NULL";
        let rows: Vec<CandidateRow> = match mode {
            OfflineMode::None => return Ok(Vec::new()),
            OfflineMode::All => sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(format!(
                "{base} ORDER BY date_unix DESC, id DESC LIMIT ?"
            )))
            .bind(mailbox.value())
            .bind(limit)
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("download_candidates_all", error))?,
            OfflineMode::Recent => {
                let cutoff = now_unix - recent_days.max(1) * SECONDS_PER_DAY;
                sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(format!(
                    "{base} AND date_unix >= ? ORDER BY date_unix DESC, id DESC LIMIT ?"
                )))
                .bind(mailbox.value())
                .bind(cutoff)
                .bind(limit)
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("download_candidates_recent", error))?
            }
        };
        rows.into_iter().map(CandidateRow::into_candidate).collect()
    }

    /// Lists cached messages (`has_raw = 1`) that fall **outside** the
    /// mailbox's current offline window and can be purged: everything for
    /// `none`, and older-than-window for `recent`. `all` yields nothing.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn cached_outside_window(
        &self,
        mailbox: MailboxId,
        now_unix: i64,
    ) -> Result<Vec<MessageId>, MailError> {
        let policy = self.get(mailbox).await?;
        let ids: Vec<i64> = match policy.mode {
            OfflineMode::All => return Ok(Vec::new()),
            OfflineMode::None => {
                sqlx::query_scalar("SELECT id FROM messages WHERE mailbox_id = ? AND has_raw = 1")
                    .bind(mailbox.value())
                    .fetch_all(self.pool)
                    .await
                    .map_err(|error| map_sqlx("cached_outside_none", error))?
            }
            OfflineMode::Recent => {
                let cutoff = now_unix - i64::from(policy.recent_days.max(1)) * SECONDS_PER_DAY;
                sqlx::query_scalar(
                    "SELECT id FROM messages \
                     WHERE mailbox_id = ? AND has_raw = 1 \
                       AND (date_unix IS NULL OR date_unix < ?)",
                )
                .bind(mailbox.value())
                .bind(cutoff)
                .fetch_all(self.pool)
                .await
                .map_err(|error| map_sqlx("cached_outside_recent", error))?
            }
        };
        Ok(ids.into_iter().map(MessageId::new).collect())
    }

    /// Flips `has_raw` to 1 and indexes the full body text for search, in one
    /// transaction. Called once a raw body has been written to the raw store.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn mark_downloaded(
        &self,
        message_id: MessageId,
        body_text: &str,
    ) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("mark_downloaded_begin", error))?;
        sqlx::query("UPDATE messages SET has_raw = 1 WHERE id = ?")
            .bind(message_id.value())
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("mark_downloaded_flag", error))?;
        crate::search::index_body(&mut *tx, message_id.value(), body_text).await?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("mark_downloaded_commit", error))?;
        Ok(())
    }

    /// Clears `has_raw` for a message whose cached body has been removed by a
    /// policy purge. Returns whether the row existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn clear_has_raw(&self, message_id: MessageId) -> Result<bool, MailError> {
        let result = sqlx::query("UPDATE messages SET has_raw = 0 WHERE id = ?")
            .bind(message_id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("clear_has_raw", error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Raw candidate row: the local id plus the remote-identity columns.
#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: i64,
    remote_kind: Option<String>,
    remote_uid: Option<i64>,
    remote_uid_validity: Option<i64>,
    remote_uidl: Option<String>,
}

impl CandidateRow {
    fn into_candidate(self) -> Result<DownloadCandidate, MailError> {
        let remote_id = decode_remote_id(
            self.remote_kind.as_deref(),
            self.remote_uid,
            self.remote_uid_validity,
            self.remote_uidl,
        )?
        .ok_or_else(|| MailError::Database {
            message: "download_candidate: message has no remote identity".to_owned(),
        })?;
        Ok(DownloadCandidate {
            message_id: MessageId::new(self.id),
            remote_id,
        })
    }
}
