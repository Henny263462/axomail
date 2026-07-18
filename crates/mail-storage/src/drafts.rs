//! Draft repository: persist, list, and delete composer drafts.
//!
//! A draft is local-only composition state for one account. Recipient
//! lists are stored as JSON text (see [`crate::address_json`]); the
//! authoritative parsed form is rebuilt at send time by the composer.

use axomail_core::{AccountId, MailError};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::address_json::{decode_addresses, encode_addresses};
use crate::error_map::map_sqlx;

/// Identifier of a persisted [`DraftRecord`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DraftId(i64);

impl DraftId {
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

impl std::fmt::Display for DraftId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Draft content to insert or update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDraft {
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
}

/// A persisted draft row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRecord {
    /// Row identifier.
    pub id: DraftId,
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
    /// Last-updated timestamp (Unix seconds).
    pub updated_unix: i64,
}

/// Columns selected when reading a draft, in [`DraftRow`] order.
const DRAFT_COLUMNS: &str =
    "id, account_id, subject, body_text, to_json, cc_json, bcc_json, updated_at_unix";

/// Repository over the `drafts` table.
#[derive(Debug)]
pub struct DraftRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> DraftRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a draft and returns the stored row with its assigned id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn insert(&self, new: &NewDraft) -> Result<DraftRecord, MailError> {
        let (to_json, cc_json, bcc_json) = encode_lists(new)?;
        let row = sqlx::query_as::<_, DraftRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO drafts (account_id, subject, body_text, to_json, cc_json, bcc_json)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING {DRAFT_COLUMNS}"
        )))
        .bind(new.account.value())
        .bind(new.subject.as_str())
        .bind(new.body_text.as_str())
        .bind(to_json)
        .bind(cc_json)
        .bind(bcc_json)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("insert_draft", error))?;

        row.into_record()
    }

    /// Overwrites the content of an existing draft, stamping the update
    /// time. Returns `true` if the draft existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update(&self, id: DraftId, new: &NewDraft) -> Result<bool, MailError> {
        let (to_json, cc_json, bcc_json) = encode_lists(new)?;
        let result = sqlx::query(
            "UPDATE drafts SET
                 account_id = ?, subject = ?, body_text = ?,
                 to_json = ?, cc_json = ?, bcc_json = ?,
                 updated_at_unix = unixepoch()
             WHERE id = ?",
        )
        .bind(new.account.value())
        .bind(new.subject.as_str())
        .bind(new.body_text.as_str())
        .bind(to_json)
        .bind(cc_json)
        .bind(bcc_json)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("update_draft", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetches a single draft by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a stored
    /// row cannot be decoded.
    pub async fn get(&self, id: DraftId) -> Result<Option<DraftRecord>, MailError> {
        let row = sqlx::query_as::<_, DraftRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {DRAFT_COLUMNS} FROM drafts WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_draft", error))?;

        row.map(DraftRow::into_record).transpose()
    }

    /// Lists every draft, most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a stored
    /// row cannot be decoded.
    pub async fn list(&self) -> Result<Vec<DraftRecord>, MailError> {
        let rows = sqlx::query_as::<_, DraftRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {DRAFT_COLUMNS} FROM drafts ORDER BY updated_at_unix DESC, id DESC"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_drafts", error))?;

        rows.into_iter().map(DraftRow::into_record).collect()
    }

    /// Deletes a draft by id. Returns `true` if a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: DraftId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM drafts WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_draft", error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Encodes the three recipient lists of a draft into their JSON columns.
fn encode_lists(new: &NewDraft) -> Result<(String, String, String), MailError> {
    Ok((
        encode_addresses(&new.to)?,
        encode_addresses(&new.cc)?,
        encode_addresses(&new.bcc)?,
    ))
}

/// Raw `drafts` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct DraftRow {
    id: i64,
    account_id: i64,
    subject: String,
    body_text: String,
    to_json: String,
    cc_json: String,
    bcc_json: String,
    updated_at_unix: i64,
}

impl DraftRow {
    fn into_record(self) -> Result<DraftRecord, MailError> {
        Ok(DraftRecord {
            id: DraftId::new(self.id),
            account: AccountId::new(self.account_id),
            subject: self.subject,
            body_text: self.body_text,
            to: decode_addresses(&self.to_json)?,
            cc: decode_addresses(&self.cc_json)?,
            bcc: decode_addresses(&self.bcc_json)?,
            updated_unix: self.updated_at_unix,
        })
    }
}
