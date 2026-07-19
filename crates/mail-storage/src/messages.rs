//! Message repository: insert envelopes with their recipients, fetch a
//! single envelope, page through a mailbox, and update flags.

use axomail_core::{
    AccountId, Address, MailError, MailboxId, MessageEnvelope, MessageFlags, MessageId,
    RemoteMessageId, ThreadingHeaders, UtcTimestamp,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error_map::map_sqlx;
use crate::message_rows::{MESSAGE_COLUMNS, MessageRow, RemoteColumns, load_recipients};
use crate::pagination::{MessageCursor, MessagePage, NULL_DATE_SORT};
use crate::threading_json::ThreadingColumns;

/// Default page size when a caller passes `0`.
const MIN_PAGE_LIMIT: u32 = 1;

/// Message envelope to insert, before an identifier has been assigned.
///
/// Carries `cc`/`bcc` in addition to the `from`/`to` of a
/// [`MessageEnvelope`] so the full recipient set is persisted; listing
/// still returns only `from`/`to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMessage {
    /// Mailbox the message belongs to.
    pub mailbox: MailboxId,
    /// Remote identity, or `None` for a local-only message.
    pub remote_id: Option<RemoteMessageId>,
    /// Decoded subject, if any.
    pub subject: Option<String>,
    /// Senders.
    pub from: Vec<Address>,
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Carbon-copy recipients.
    pub cc: Vec<Address>,
    /// Blind-carbon-copy recipients.
    pub bcc: Vec<Address>,
    /// Date header, if parseable.
    pub date: Option<UtcTimestamp>,
    /// Flags.
    pub flags: MessageFlags,
    /// Full size in bytes, if known.
    pub size_bytes: Option<u64>,
    /// Sanitized plain-text preview.
    pub preview: Option<String>,
    /// Whether the raw RFC 5322 bytes are stored outside the database.
    pub has_raw: bool,
}

/// Repository over the `messages` and `message_recipients` tables.
#[derive(Debug)]
pub struct MessageRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MessageRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a message and its recipients in one transaction and
    /// returns the resulting envelope with its assigned identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if
    /// `size_bytes` exceeds [`i64::MAX`].
    pub async fn insert(&self, new: &NewMessage) -> Result<MessageEnvelope, MailError> {
        let remote = RemoteColumns::from_remote_id(new.remote_id.as_ref());
        let date = new.date.map(UtcTimestamp::unix_seconds);
        let size =
            new.size_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(|_| MailError::Database {
                    message: "insert_message: size out of range".to_owned(),
                })?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("insert_message", error))?;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO messages (
                mailbox_id, remote_kind, remote_uid, remote_uid_validity, remote_uidl,
                subject, date_unix,
                flag_seen, flag_answered, flag_flagged, flag_draft, flag_deleted,
                size_bytes, preview, has_raw
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id",
        )
        .bind(new.mailbox.value())
        .bind(remote.kind)
        .bind(remote.uid)
        .bind(remote.uid_validity)
        .bind(remote.uidl.as_deref())
        .bind(new.subject.as_deref())
        .bind(date)
        .bind(new.flags.seen)
        .bind(new.flags.answered)
        .bind(new.flags.flagged)
        .bind(new.flags.draft)
        .bind(new.flags.deleted)
        .bind(size)
        .bind(new.preview.as_deref())
        .bind(new.has_raw)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| map_sqlx("insert_message", error))?;

        insert_recipients(&mut tx, id, "from", &new.from).await?;
        insert_recipients(&mut tx, id, "to", &new.to).await?;
        insert_recipients(&mut tx, id, "cc", &new.cc).await?;
        insert_recipients(&mut tx, id, "bcc", &new.bcc).await?;

        // Index header text plus the preview (as the initial body) so the
        // message is searchable right after insertion.
        let correspondents = crate::search::correspondents_text(
            new.from
                .iter()
                .chain(&new.to)
                .chain(&new.cc)
                .chain(&new.bcc),
        );
        crate::search::upsert_content(
            &mut tx,
            id,
            new.subject.as_deref(),
            &correspondents,
            new.preview.as_deref().unwrap_or_default(),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| map_sqlx("insert_message", error))?;

        Ok(MessageEnvelope {
            id: MessageId::new(id),
            mailbox: new.mailbox,
            remote_id: new.remote_id.clone(),
            subject: new.subject.clone(),
            from: new.from.clone(),
            to: new.to.clone(),
            date: new.date,
            flags: new.flags.clone(),
            size_bytes: new.size_bytes,
            preview: new.preview.clone(),
            // Local inserts carry no threading identity; a reply draft gains
            // one only once it is sent and re-synced.
            threading: ThreadingHeaders::default(),
        })
    }

    /// Fetches a single envelope by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a
    /// stored row cannot be decoded.
    pub async fn get(&self, id: MessageId) -> Result<Option<MessageEnvelope>, MailError> {
        let row = sqlx::query_as::<_, MessageRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_message", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut recipients = load_recipients(self.pool, &[row.id]).await?;
        let (from, to) = recipients.remove(&row.id).unwrap_or_default();
        Ok(Some(row.into_envelope(from, to)?))
    }

    /// Returns the ids of every message belonging to `account`, across all its
    /// mailboxes.
    ///
    /// Used to reclaim out-of-database resources keyed by message id (the
    /// cached raw `.eml` bodies) before an account row is deleted and its
    /// message rows cascade away.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn list_ids_for_account(
        &self,
        account: AccountId,
    ) -> Result<Vec<MessageId>, MailError> {
        let ids = sqlx::query_scalar::<_, i64>(
            "SELECT m.id FROM messages m \
             JOIN mailboxes mb ON mb.id = m.mailbox_id \
             WHERE mb.account_id = ?",
        )
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_message_ids_for_account", error))?;
        Ok(ids.into_iter().map(MessageId::new).collect())
    }

    /// Returns one page of a mailbox's messages ordered by
    /// `(date DESC, id DESC)`, plus a cursor for the next page.
    ///
    /// Pass `cursor = None` for the first page and the returned
    /// [`MessagePage::next`] for each following page. A `limit` of `0`
    /// is treated as `1`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a
    /// stored row cannot be decoded.
    pub async fn list_page(
        &self,
        mailbox: MailboxId,
        cursor: Option<MessageCursor>,
        limit: u32,
    ) -> Result<MessagePage, MailError> {
        let limit = limit.max(MIN_PAGE_LIMIT);
        let fetch = i64::from(limit) + 1;

        let mut sql = format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE mailbox_id = ?");
        if cursor.is_some() {
            sql.push_str(
                " AND (COALESCE(date_unix, ?) < ? \
                   OR (COALESCE(date_unix, ?) = ? AND id < ?))",
            );
        }
        sql.push_str(" ORDER BY COALESCE(date_unix, ?) DESC, id DESC LIMIT ?");

        let mut query =
            sqlx::query_as::<_, MessageRow>(sqlx::AssertSqlSafe(sql)).bind(mailbox.value());
        if let Some(cursor) = cursor {
            query = query
                .bind(NULL_DATE_SORT)
                .bind(cursor.sort_date())
                .bind(NULL_DATE_SORT)
                .bind(cursor.sort_date())
                .bind(cursor.id());
        }
        query = query.bind(NULL_DATE_SORT).bind(fetch);

        let mut rows = query
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("list_messages", error))?;

        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);

        let next = if has_more {
            rows.last()
                .map(|row| MessageCursor::new(row.sort_date(), row.id))
        } else {
            None
        };

        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let mut recipients = load_recipients(self.pool, &ids).await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let (from, to) = recipients.remove(&row.id).unwrap_or_default();
            items.push(row.into_envelope(from, to)?);
        }

        Ok(MessagePage { items, next })
    }

    /// Overwrites a message's flags. Returns `true` if the message
    /// exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update_flags(
        &self,
        id: MessageId,
        flags: &MessageFlags,
    ) -> Result<bool, MailError> {
        let result = sqlx::query(
            "UPDATE messages SET
                 flag_seen = ?, flag_answered = ?, flag_flagged = ?,
                 flag_draft = ?, flag_deleted = ?
             WHERE id = ?",
        )
        .bind(flags.seen)
        .bind(flags.answered)
        .bind(flags.flagged)
        .bind(flags.draft)
        .bind(flags.deleted)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("update_flags", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Finds the local id of a stored message whose `Message-ID` matches
    /// `message_id` within `account`, if any. Used by thread assembly and
    /// reply matching. Backed by the partial `idx_messages_message_id`
    /// index (migration 0006); ties (the same id synced into several
    /// mailboxes) resolve to the lowest local id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn find_by_message_id(
        &self,
        account: AccountId,
        message_id: &str,
    ) -> Result<Option<MessageId>, MailError> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT m.id FROM messages m
             JOIN mailboxes b ON b.id = m.mailbox_id
             WHERE b.account_id = ? AND m.message_id = ?
             ORDER BY m.id
             LIMIT 1",
        )
        .bind(account.value())
        .bind(message_id)
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("find_by_message_id", error))?;
        Ok(id.map(MessageId::new))
    }

    /// Persists a message's extracted threading identity, but only if it
    /// has not been extracted yet (`references_json IS NULL`). Returns
    /// whether a row was updated.
    ///
    /// This is the lazy-backfill write: the first time a message is opened
    /// and its raw body parsed, its Message-ID / In-Reply-To / References
    /// are stored. The `references_json IS NULL` guard makes it idempotent,
    /// so re-opening a message (or a message already populated at ingest)
    /// is a no-op and never overwrites an existing identity.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if the
    /// references chain cannot be serialized.
    pub async fn backfill_threading(
        &self,
        id: MessageId,
        headers: &ThreadingHeaders,
    ) -> Result<bool, MailError> {
        let columns = ThreadingColumns::from_headers(headers)?;
        let result = sqlx::query(
            "UPDATE messages
                 SET message_id = ?, in_reply_to = ?, references_json = ?
             WHERE id = ? AND references_json IS NULL",
        )
        .bind(columns.message_id)
        .bind(columns.in_reply_to)
        .bind(columns.references_json)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("backfill_threading", error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Inserts one kind of recipient list within an open transaction,
/// preserving order via the `position` column.
async fn insert_recipients(
    conn: &mut SqliteConnection,
    message_id: i64,
    kind: &str,
    addresses: &[Address],
) -> Result<(), MailError> {
    for (position, address) in addresses.iter().enumerate() {
        sqlx::query(
            "INSERT INTO message_recipients (message_id, kind, display_name, email, position)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(message_id)
        .bind(kind)
        .bind(address.display_name.as_deref())
        .bind(address.email.as_str())
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("insert_recipient", error))?;
    }
    Ok(())
}
