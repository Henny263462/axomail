//! Tag repository: per-account tags (the local projection of IMAP keywords)
//! and their message associations.
//!
//! A tag is created three ways: seeded on account creation, added by the user
//! in the tag manager, or auto-created during sync when the server reports an
//! unknown keyword (see [`sync`]). Every tag carries a canonical
//! [`axomail_core::Keyword`]; the keyword is the tag's identity within its
//! account (unique, case-insensitive). The repository exposes CRUD, the
//! per-message and per-tag association reads, and the batched tags-for-a-page
//! read the message list uses for row chips without an N+1 query.

mod types;

pub(crate) mod sync;

use std::collections::HashMap;

use axomail_core::{AccountId, Keyword, MailError, MessageEnvelope, MessageId};
use sqlx::SqlitePool;

pub use types::{NewTag, Tag, TagColor, TagId, TagUpdate};

use crate::error_map::map_sqlx;
use crate::message_rows::{MESSAGE_COLUMNS_M, MessageRow, load_recipients};
use crate::pagination::NULL_DATE_SORT;

/// Columns selected when reading a tag, in [`TagRow`] order.
const TAG_COLUMNS: &str = "id, account_id, keyword, display_name, color, sort_order";

/// [`TAG_COLUMNS`] qualified with the `t` alias, for joins against `tags`.
const TAG_COLUMNS_T: &str = "t.id, t.account_id, t.keyword, t.display_name, t.color, t.sort_order";

/// Repository over the `tags` and `message_tags` tables.
#[derive(Debug)]
pub struct TagRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> TagRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a tag and returns it with its assigned id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure, including the
    /// unique-keyword-per-account constraint if the keyword already exists.
    pub async fn create(&self, new: &NewTag) -> Result<Tag, MailError> {
        let row = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO tags (account_id, keyword, display_name, color, sort_order)
             VALUES (?, ?, ?, ?, ?)
             RETURNING {TAG_COLUMNS}"
        )))
        .bind(new.account.value())
        .bind(new.keyword.as_str())
        .bind(new.display_name.as_str())
        .bind(new.color.as_ref().map(TagColor::as_str))
        .bind(new.sort_order)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("create_tag", error))?;
        row.into_tag()
    }

    /// Overwrites a tag's display name and color (its keyword and account are
    /// immutable). Returns the updated tag, or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn update(&self, id: TagId, update: &TagUpdate) -> Result<Option<Tag>, MailError> {
        let row = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE tags SET display_name = ?, color = ? WHERE id = ?
             RETURNING {TAG_COLUMNS}"
        )))
        .bind(update.display_name.as_str())
        .bind(update.color.as_ref().map(TagColor::as_str))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("update_tag", error))?;
        row.map(TagRow::into_tag).transpose()
    }

    /// Deletes a tag, cascading to its message associations. Returns `true` if
    /// a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: TagId) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_tag", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetches a tag by id, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get(&self, id: TagId) -> Result<Option<Tag>, MailError> {
        let row = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS} FROM tags WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_tag", error))?;
        row.map(TagRow::into_tag).transpose()
    }

    /// Lists an account's tags in display order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list(&self, account: AccountId) -> Result<Vec<Tag>, MailError> {
        let rows = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS} FROM tags WHERE account_id = ? ORDER BY sort_order, id"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_tags", error))?;
        rows.into_iter().map(TagRow::into_tag).collect()
    }

    /// Lists every tag across all accounts with its message-usage count, in
    /// account then display order, for the cross-account tag filter sidebar.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list_all_with_usage(&self) -> Result<Vec<(Tag, i64)>, MailError> {
        let rows = sqlx::query_as::<_, TagUsageRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS_T}, COUNT(mt.message_id) AS usage_count
             FROM tags t
             LEFT JOIN message_tags mt ON mt.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.account_id, t.sort_order, t.id"
        )))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_all_tags_with_usage", error))?;
        rows.into_iter()
            .map(|row| Ok((row.tag.into_tag()?, row.usage_count)))
            .collect()
    }

    /// Lists an account's tags with the number of messages carrying each, for
    /// the tag manager's in-use warning.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list_with_usage(&self, account: AccountId) -> Result<Vec<(Tag, i64)>, MailError> {
        let rows = sqlx::query_as::<_, TagUsageRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS_T}, COUNT(mt.message_id) AS usage_count
             FROM tags t
             LEFT JOIN message_tags mt ON mt.tag_id = t.id
             WHERE t.account_id = ?
             GROUP BY t.id
             ORDER BY t.sort_order, t.id"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_tags_with_usage", error))?;
        rows.into_iter()
            .map(|row| Ok((row.tag.into_tag()?, row.usage_count)))
            .collect()
    }

    /// Counts the messages carrying `tag`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn usage_count(&self, tag: TagId) -> Result<i64, MailError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_tags WHERE tag_id = ?")
            .bind(tag.value())
            .fetch_one(self.pool)
            .await
            .map_err(|error| map_sqlx("tag_usage_count", error))?;
        Ok(count)
    }

    /// Finds an account's tag for a keyword, if one exists.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn find_by_keyword(
        &self,
        account: AccountId,
        keyword: &Keyword,
    ) -> Result<Option<Tag>, MailError> {
        let row = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS} FROM tags WHERE account_id = ? AND keyword = ?"
        )))
        .bind(account.value())
        .bind(keyword.as_str())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("find_tag_by_keyword", error))?;
        row.map(TagRow::into_tag).transpose()
    }

    /// Returns the tags applied to one message, in display order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn tags_for_message(&self, message: MessageId) -> Result<Vec<Tag>, MailError> {
        let rows = sqlx::query_as::<_, TagRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {TAG_COLUMNS_T} FROM tags t
             JOIN message_tags mt ON mt.tag_id = t.id
             WHERE mt.message_id = ?
             ORDER BY t.sort_order, t.id"
        )))
        .bind(message.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("tags_for_message", error))?;
        rows.into_iter().map(TagRow::into_tag).collect()
    }

    /// Returns the tags for a set of messages in one query, grouped by message
    /// id. The message list uses this to render row chips in O(page), never a
    /// per-row lookup. An empty id slice touches no database.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn tags_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<i64, Vec<Tag>>, MailError> {
        let mut grouped: HashMap<i64, Vec<Tag>> = HashMap::new();
        if message_ids.is_empty() {
            return Ok(grouped);
        }
        let placeholders = vec!["?"; message_ids.len()].join(", ");
        let sql = format!(
            "SELECT mt.message_id AS message_id, {TAG_COLUMNS_T} FROM tags t
             JOIN message_tags mt ON mt.tag_id = t.id
             WHERE mt.message_id IN ({placeholders})
             ORDER BY mt.message_id, t.sort_order, t.id"
        );
        let mut query = sqlx::query_as::<_, MessageTagRow>(sqlx::AssertSqlSafe(sql));
        for id in message_ids {
            query = query.bind(id.value());
        }
        let rows = query
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("tags_for_messages", error))?;
        for row in rows {
            grouped
                .entry(row.message_id)
                .or_default()
                .push(row.tag.into_tag()?);
        }
        Ok(grouped)
    }

    /// Returns the envelopes of the messages carrying `tag`, newest first, up
    /// to `limit`, for the tag filter view.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn messages_for_tag(
        &self,
        tag: TagId,
        limit: u32,
    ) -> Result<Vec<MessageEnvelope>, MailError> {
        let rows = sqlx::query_as::<_, MessageRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {MESSAGE_COLUMNS_M} FROM messages m
             JOIN message_tags mt ON mt.message_id = m.id
             WHERE mt.tag_id = ?
             ORDER BY COALESCE(m.date_unix, ?) DESC, m.id DESC
             LIMIT ?"
        )))
        .bind(tag.value())
        .bind(NULL_DATE_SORT)
        .bind(i64::from(limit.max(1)))
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("messages_for_tag", error))?;

        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let mut recipients = load_recipients(self.pool, &ids).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let (from, to) = recipients.remove(&row.id).unwrap_or_default();
            items.push(row.into_envelope(from, to)?);
        }
        Ok(items)
    }

    /// Links a message to a tag. Returns `true` if a new association was made
    /// (false if it already existed).
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn add_message_tag(&self, message: MessageId, tag: TagId) -> Result<bool, MailError> {
        let result = sqlx::query(
            "INSERT INTO message_tags (message_id, tag_id) VALUES (?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(message.value())
        .bind(tag.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("add_message_tag", error))?;
        Ok(result.rows_affected() > 0)
    }

    /// Unlinks a message from a tag. Returns `true` if an association existed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn remove_message_tag(
        &self,
        message: MessageId,
        tag: TagId,
    ) -> Result<bool, MailError> {
        let result = sqlx::query("DELETE FROM message_tags WHERE message_id = ? AND tag_id = ?")
            .bind(message.value())
            .bind(tag.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("remove_message_tag", error))?;
        Ok(result.rows_affected() > 0)
    }
}

/// Raw `tags` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct TagRow {
    id: i64,
    account_id: i64,
    keyword: String,
    display_name: String,
    color: Option<String>,
    sort_order: i64,
}

impl TagRow {
    fn into_tag(self) -> Result<Tag, MailError> {
        let color = self
            .color
            .map(|value| TagColor::parse(&value))
            .transpose()?;
        Ok(Tag {
            id: TagId::new(self.id),
            account: AccountId::new(self.account_id),
            keyword: Keyword::parse(&self.keyword)?,
            display_name: self.display_name,
            color,
            sort_order: self.sort_order,
        })
    }
}

/// A `tags` row joined with its message id (for the batched page read).
#[derive(sqlx::FromRow)]
struct MessageTagRow {
    message_id: i64,
    #[sqlx(flatten)]
    tag: TagRow,
}

/// A `tags` row joined with its message-usage count (for the tag manager).
#[derive(sqlx::FromRow)]
struct TagUsageRow {
    #[sqlx(flatten)]
    tag: TagRow,
    usage_count: i64,
}
