//! Unified inbox: one date-ordered page over the inbox-role mailboxes of
//! every account.
//!
//! The listing reuses the message keyset [`MessageCursor`] and the same
//! `(date DESC, id DESC)` ordering as [`crate::MessageRepo::list_page`],
//! so cursors are interchangeable in shape. Each row additionally carries
//! the owning [`AccountId`], letting the UI badge which account a message
//! came from without a second lookup.

use axomail_core::{AccountId, MailError, MessageEnvelope};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;
use crate::message_rows::{MESSAGE_COLUMNS_M, MessageRow, load_recipients};
use crate::pagination::{MessageCursor, NULL_DATE_SORT};

/// Default page size when a caller passes `0`.
const MIN_PAGE_LIMIT: u32 = 1;

/// One unified-inbox entry: a message envelope plus the account it
/// belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedMessage {
    /// Account whose inbox the message lives in.
    pub account: AccountId,
    /// The message envelope, identical in shape to a per-mailbox listing.
    pub envelope: MessageEnvelope,
}

/// One page of the unified inbox plus the cursor for the following page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedInboxPage {
    /// Entries in this page, newest first across all accounts.
    pub items: Vec<UnifiedMessage>,
    /// Cursor for the next page, or `None` if this is the last page.
    pub next: Option<MessageCursor>,
}

/// Repository for the cross-account unified inbox view.
#[derive(Debug)]
pub struct UnifiedRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> UnifiedRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Returns one page of every account's inbox messages, ordered by
    /// `(date DESC, id DESC)`, plus a cursor for the next page.
    ///
    /// Pass `cursor = None` for the first page and the returned
    /// [`UnifiedInboxPage::next`] for each following page. A `limit` of
    /// `0` is treated as `1`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or if a stored
    /// row cannot be decoded.
    pub async fn list_unified_inbox(
        &self,
        cursor: Option<MessageCursor>,
        limit: u32,
    ) -> Result<UnifiedInboxPage, MailError> {
        let limit = limit.max(MIN_PAGE_LIMIT);
        let fetch = i64::from(limit) + 1;

        let mut sql = format!(
            "SELECT mb.account_id AS account_id, {MESSAGE_COLUMNS_M} \
             FROM messages m \
             JOIN mailboxes mb ON mb.id = m.mailbox_id \
             WHERE mb.role = 'inbox'"
        );
        if cursor.is_some() {
            sql.push_str(
                " AND (COALESCE(m.date_unix, ?) < ? \
                   OR (COALESCE(m.date_unix, ?) = ? AND m.id < ?))",
            );
        }
        sql.push_str(" ORDER BY COALESCE(m.date_unix, ?) DESC, m.id DESC LIMIT ?");

        let mut query = sqlx::query_as::<_, UnifiedRow>(sqlx::AssertSqlSafe(sql));
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
            .map_err(|error| map_sqlx("list_unified_inbox", error))?;

        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);

        let next = if has_more {
            rows.last()
                .map(|row| MessageCursor::new(row.message.sort_date(), row.message.id))
        } else {
            None
        };

        let ids: Vec<i64> = rows.iter().map(|row| row.message.id).collect();
        let mut recipients = load_recipients(self.pool, &ids).await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let (from, to) = recipients.remove(&row.message.id).unwrap_or_default();
            items.push(UnifiedMessage {
                account: AccountId::new(row.account_id),
                envelope: row.message.into_envelope(from, to)?,
            });
        }

        Ok(UnifiedInboxPage { items, next })
    }
}

/// Raw unified-inbox row: the owning account id plus a flattened
/// [`MessageRow`] built from the joined `messages` columns.
#[derive(sqlx::FromRow)]
struct UnifiedRow {
    account_id: i64,
    #[sqlx(flatten)]
    message: MessageRow,
}
