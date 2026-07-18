//! Outbox repository: the outgoing send queue.
//!
//! A row moves `queued -> sending -> {sent, failed}`. The transition into
//! `sending` is a single guarded `UPDATE` ([`OutboxRepo::begin_send`]) so a
//! row already `sending` or `sent` can never be submitted twice — the
//! composer's duplicate-send protection. Recipient lists are stored as JSON
//! text (see [`crate::address_json`]).

use axomail_core::{AccountId, MailError};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::address_json::{decode_addresses, encode_addresses};
use crate::drafts::DraftId;
use crate::error_map::map_sqlx;

/// Identifier of a persisted [`OutboxRecord`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutboxId(i64);

impl OutboxId {
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

impl std::fmt::Display for OutboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lifecycle state of an outbox row, matching the schema's `state` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxState {
    /// Waiting to be submitted (the initial state).
    Queued,
    /// A submission attempt is in progress.
    Sending,
    /// Accepted by the submission server.
    Sent,
    /// The last attempt failed; carries a redacted reason.
    Failed,
}

impl OutboxState {
    /// Serialized form stored in the `state` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "queued" => Ok(Self::Queued),
            "sending" => Ok(Self::Sending),
            "sent" => Ok(Self::Sent),
            "failed" => Ok(Self::Failed),
            _ => Err(MailError::Database {
                message: "read_outbox: unknown state".to_owned(),
            }),
        }
    }
}

/// Outgoing message content to enqueue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewOutbox {
    /// Owning account.
    pub account: AccountId,
    /// Draft this message originated from, if any.
    pub draft_id: Option<DraftId>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub body_text: String,
    /// Primary recipients as address strings.
    pub to: Vec<String>,
    /// Carbon-copy recipients as address strings.
    pub cc: Vec<String>,
    /// Blind-carbon-copy recipients as address strings.
    pub bcc: Vec<String>,
}

/// A persisted outbox row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRecord {
    /// Row identifier.
    pub id: OutboxId,
    /// Owning account.
    pub account: AccountId,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub body_text: String,
    /// Primary recipients as address strings.
    pub to: Vec<String>,
    /// Carbon-copy recipients as address strings.
    pub cc: Vec<String>,
    /// Blind-carbon-copy recipients as address strings.
    pub bcc: Vec<String>,
    /// Current lifecycle state.
    pub state: OutboxState,
    /// Number of submission attempts made so far.
    pub attempts: i64,
    /// Redacted last failure reason, if any.
    pub last_error: Option<String>,
    /// Last-updated timestamp (Unix seconds).
    pub updated_unix: i64,
}

/// Columns selected when reading an outbox row, in [`OutboxRow`] order.
const OUTBOX_COLUMNS: &str = "id, account_id, subject, body_text, \
    to_json, cc_json, bcc_json, state, attempts, last_error, updated_at_unix";

/// Repository over the `outbox` table.
#[derive(Debug)]
pub struct OutboxRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> OutboxRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a `queued` outbox row and returns it with its assigned id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create(&self, new: &NewOutbox) -> Result<OutboxRecord, MailError> {
        let to_json = encode_addresses(&new.to)?;
        let cc_json = encode_addresses(&new.cc)?;
        let bcc_json = encode_addresses(&new.bcc)?;
        let row = sqlx::query_as::<_, OutboxRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO outbox (account_id, draft_id, subject, body_text, to_json, cc_json, bcc_json)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             RETURNING {OUTBOX_COLUMNS}"
        )))
        .bind(new.account.value())
        .bind(new.draft_id.map(DraftId::value))
        .bind(new.subject.as_str())
        .bind(new.body_text.as_str())
        .bind(to_json)
        .bind(cc_json)
        .bind(bcc_json)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("create_outbox", error))?;

        row.into_record()
    }

    /// Atomically transitions a row into `sending`, incrementing its
    /// attempt count, but only from `queued` or `failed`.
    ///
    /// Returns `true` if the transition happened. A row already `sending`
    /// or `sent` (or absent) yields `false`: this guard is the composer's
    /// duplicate-send protection, so a `false` result means the caller must
    /// **not** submit the message.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn begin_send(&self, id: OutboxId) -> Result<bool, MailError> {
        let result = sqlx::query(
            "UPDATE outbox
                SET state = 'sending', attempts = attempts + 1, updated_at_unix = unixepoch()
             WHERE id = ? AND state IN ('queued', 'failed')",
        )
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("begin_send_outbox", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Records a successful submission, clearing any prior error.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn mark_sent(&self, id: OutboxId) -> Result<(), MailError> {
        sqlx::query(
            "UPDATE outbox SET state = 'sent', last_error = NULL, updated_at_unix = unixepoch()
             WHERE id = ?",
        )
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("mark_outbox_sent", error))?;
        Ok(())
    }

    /// Records a failed submission with a redacted reason.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn mark_failed(&self, id: OutboxId, error: &str) -> Result<(), MailError> {
        sqlx::query(
            "UPDATE outbox SET state = 'failed', last_error = ?, updated_at_unix = unixepoch()
             WHERE id = ?",
        )
        .bind(error)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("mark_outbox_failed", error))?;
        Ok(())
    }

    /// Fetches a single outbox row by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a stored
    /// row cannot be decoded.
    pub async fn get(&self, id: OutboxId) -> Result<Option<OutboxRecord>, MailError> {
        let row = sqlx::query_as::<_, OutboxRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OUTBOX_COLUMNS} FROM outbox WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_outbox", error))?;

        row.map(OutboxRow::into_record).transpose()
    }

    /// Lists every outbox row, most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a stored
    /// row cannot be decoded.
    pub async fn list(&self) -> Result<Vec<OutboxRecord>, MailError> {
        let rows = sqlx::query_as::<_, OutboxRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {OUTBOX_COLUMNS} FROM outbox ORDER BY updated_at_unix DESC, id DESC"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_outbox", error))?;

        rows.into_iter().map(OutboxRow::into_record).collect()
    }
}

/// Raw `outbox` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: i64,
    account_id: i64,
    subject: String,
    body_text: String,
    to_json: String,
    cc_json: String,
    bcc_json: String,
    state: String,
    attempts: i64,
    last_error: Option<String>,
    updated_at_unix: i64,
}

impl OutboxRow {
    fn into_record(self) -> Result<OutboxRecord, MailError> {
        Ok(OutboxRecord {
            id: OutboxId::new(self.id),
            account: AccountId::new(self.account_id),
            subject: self.subject,
            body_text: self.body_text,
            to: decode_addresses(&self.to_json)?,
            cc: decode_addresses(&self.cc_json)?,
            bcc: decode_addresses(&self.bcc_json)?,
            state: OutboxState::parse(&self.state)?,
            attempts: self.attempts,
            last_error: self.last_error,
            updated_unix: self.updated_at_unix,
        })
    }
}
