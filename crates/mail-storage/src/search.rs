//! Full-text search over message headers and body text (ADR-0008).
//!
//! Searchable text is denormalized into the `message_search_content`
//! table (subject, sender/recipient names and addresses, extracted body)
//! which backs the `messages_fts` external-content FTS5 index. Triggers
//! on the content table keep the index in sync inside the writing
//! transaction, so message inserts and deletes propagate automatically
//! (deletes cascade from `messages`, firing the FTS delete trigger).
//!
//! User queries are never interpolated into SQL: [`build_match_query`]
//! turns free text into a safe FTS5 `MATCH` expression that is
//! parameter-bound, quoting every token so operators such as `AND`, `OR`,
//! and parentheses are searched as literals, while a trailing `*` is kept
//! as a prefix search.

use axomail_core::{AccountId, Address, MailError, MailboxId, MessageId};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error_map::map_sqlx;

/// One search result: which message matched, the mailbox it lives in, and
/// a highlighted snippet of the matching text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Identifier of the matching message.
    pub message_id: MessageId,
    /// Mailbox the matching message belongs to.
    pub mailbox_id: MailboxId,
    /// Snippet of the matching text with `[`/`]` around matched terms.
    pub snippet: String,
}

/// Repository over the search index (`messages_fts` and its content and
/// bookkeeping tables).
#[derive(Debug)]
pub struct SearchRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SearchRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Indexes (or re-indexes) the extracted plain-text body of a
    /// message. Called by the message-detail path once a body has been
    /// parsed; header text (subject and correspondents) is indexed
    /// earlier, at ingest time.
    ///
    /// Creates the content row if it does not exist yet, preserving any
    /// previously indexed header text.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure, including a
    /// `message_id` that does not refer to a stored message.
    pub async fn index_message_body(
        &self,
        message_id: MessageId,
        text: &str,
    ) -> Result<(), MailError> {
        index_body(self.pool, message_id.value(), text).await
    }

    /// Full-text searches the index, ranking matches with BM25 (subject
    /// weighted highest, then correspondents, then body).
    ///
    /// When `account_ids` is empty the search spans every account;
    /// otherwise it is restricted to the listed accounts. A query that
    /// contains no searchable tokens yields an empty result rather than an
    /// error. `limit` of `0` is treated as `1`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn search(
        &self,
        account_ids: &[AccountId],
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchHit>, MailError> {
        let Some(match_query) = build_match_query(query) else {
            return Ok(Vec::new());
        };
        let limit = i64::from(limit.max(1));

        let mut sql = String::from(
            "SELECT m.id AS message_id, m.mailbox_id AS mailbox_id, \
                 snippet(messages_fts, -1, '[', ']', '…', 10) AS snippet \
             FROM messages_fts \
             JOIN messages m ON m.id = messages_fts.rowid \
             JOIN mailboxes mb ON mb.id = m.mailbox_id \
             WHERE messages_fts MATCH ?",
        );
        if !account_ids.is_empty() {
            let placeholders = vec!["?"; account_ids.len()].join(", ");
            sql.push_str(" AND mb.account_id IN (");
            sql.push_str(&placeholders);
            sql.push(')');
        }
        sql.push_str(" ORDER BY bm25(messages_fts, 10.0, 5.0, 1.0) LIMIT ?");

        let mut query =
            sqlx::query_as::<_, SearchHitRow>(sqlx::AssertSqlSafe(sql)).bind(match_query);
        for account in account_ids {
            query = query.bind(account.value());
        }
        query = query.bind(limit);

        let rows = query
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("search", error))?;
        Ok(rows.into_iter().map(SearchHitRow::into_hit).collect())
    }

    /// Records how far body indexing has progressed for a mailbox so an
    /// interrupted rebuild can resume from just after `last_indexed`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_index_progress(
        &self,
        mailbox: MailboxId,
        last_indexed: MessageId,
    ) -> Result<(), MailError> {
        sqlx::query(
            "INSERT INTO search_index_state
                 (mailbox_id, last_indexed_message_id, last_indexed_unix, pending)
             VALUES (?, ?, unixepoch(), 0)
             ON CONFLICT (mailbox_id) DO UPDATE SET
                 last_indexed_message_id = excluded.last_indexed_message_id,
                 last_indexed_unix = excluded.last_indexed_unix,
                 pending = 0",
        )
        .bind(mailbox.value())
        .bind(last_indexed.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("set_index_progress", error))?;
        Ok(())
    }

    /// Returns the last message id whose body was indexed for a mailbox,
    /// or `None` if indexing has not started there. A resuming rebuild
    /// processes messages with a larger id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn index_progress(&self, mailbox: MailboxId) -> Result<Option<MessageId>, MailError> {
        let value: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT last_indexed_message_id FROM search_index_state WHERE mailbox_id = ?",
        )
        .bind(mailbox.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("index_progress", error))?;
        Ok(value.flatten().map(MessageId::new))
    }
}

/// Indexes (or re-indexes) a message's extracted plain-text body against any
/// executor — the pool for the on-open reindex, or an open transaction for the
/// sync-time offline download that flips `has_raw` and indexes the body
/// atomically. Creates the content row if absent, preserving header text.
pub(crate) async fn index_body<'e, E>(
    executor: E,
    message_id: i64,
    text: &str,
) -> Result<(), MailError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO message_search_content (message_id, subject, correspondents, body)
         VALUES (?, '', '', ?)
         ON CONFLICT (message_id) DO UPDATE SET body = excluded.body",
    )
    .bind(message_id)
    .bind(text)
    .execute(executor)
    .await
    .map_err(|error| map_sqlx("index_message_body", error))?;
    Ok(())
}

/// Upserts the searchable content row for a message within an open
/// transaction. `body` may be empty when only header text is known yet;
/// [`SearchRepo::index_message_body`] fills it in later. An empty `body`
/// never overwrites an already indexed one.
pub(crate) async fn upsert_content(
    conn: &mut SqliteConnection,
    message_id: i64,
    subject: Option<&str>,
    correspondents: &str,
    body: &str,
) -> Result<(), MailError> {
    sqlx::query(
        "INSERT INTO message_search_content (message_id, subject, correspondents, body)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (message_id) DO UPDATE SET
             subject = excluded.subject,
             correspondents = excluded.correspondents,
             body = CASE WHEN excluded.body <> ''
                         THEN excluded.body ELSE message_search_content.body END",
    )
    .bind(message_id)
    .bind(subject.unwrap_or_default())
    .bind(correspondents)
    .bind(body)
    .execute(&mut *conn)
    .await
    .map_err(|error| map_sqlx("index_message_content", error))?;
    Ok(())
}

/// Builds the denormalized correspondents text (display names and
/// addresses) indexed for sender/recipient search.
pub(crate) fn correspondents_text<'a, I>(addresses: I) -> String
where
    I: IntoIterator<Item = &'a Address>,
{
    let mut parts: Vec<&str> = Vec::new();
    for address in addresses {
        if let Some(name) = address.display_name.as_deref() {
            parts.push(name);
        }
        parts.push(address.email.as_str());
    }
    parts.join(" ")
}

/// Turns free-text user input into a safe FTS5 `MATCH` expression, or
/// `None` when the input carries no searchable token.
///
/// Each whitespace-separated token becomes a double-quoted FTS5 string
/// (embedded quotes doubled), so operator keywords (`AND`, `OR`, `NOT`,
/// `NEAR`), parentheses, colons, and other syntax characters are matched
/// as ordinary text. A single trailing `*` on a token is preserved as a
/// prefix search. Tokens with no tokenizable (alphanumeric) character are
/// dropped so the expression never degenerates into an empty phrase.
fn build_match_query(raw: &str) -> Option<String> {
    let mut phrases: Vec<String> = Vec::new();
    for token in raw.split_whitespace() {
        let (core, prefix) = match token.strip_suffix('*') {
            Some(stripped) => (stripped, true),
            None => (token, false),
        };
        if !core.chars().any(char::is_alphanumeric) {
            continue;
        }
        let escaped = core.replace('"', "\"\"");
        if prefix {
            phrases.push(format!("\"{escaped}\"*"));
        } else {
            phrases.push(format!("\"{escaped}\""));
        }
    }
    if phrases.is_empty() {
        None
    } else {
        Some(phrases.join(" "))
    }
}

/// Raw search-result row.
#[derive(sqlx::FromRow)]
struct SearchHitRow {
    message_id: i64,
    mailbox_id: i64,
    snippet: String,
}

impl SearchHitRow {
    fn into_hit(self) -> SearchHit {
        SearchHit {
            message_id: MessageId::new(self.message_id),
            mailbox_id: MailboxId::new(self.mailbox_id),
            snippet: self.snippet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_match_query;

    #[test]
    fn quotes_plain_tokens_and_joins_them() {
        assert_eq!(
            build_match_query("hello world").as_deref(),
            Some("\"hello\" \"world\"")
        );
    }

    #[test]
    fn operators_and_punctuation_become_literals() {
        // AND/OR/NOT and parentheses must be quoted, not treated as FTS5
        // syntax; an embedded quote is doubled.
        assert_eq!(
            build_match_query("foo AND (bar)").as_deref(),
            Some("\"foo\" \"AND\" \"(bar)\"")
        );
        assert_eq!(build_match_query("a\"b").as_deref(), Some("\"a\"\"b\""));
    }

    #[test]
    fn trailing_asterisk_is_a_prefix() {
        assert_eq!(build_match_query("wor*").as_deref(), Some("\"wor\"*"));
    }

    #[test]
    fn tokenless_input_yields_none() {
        assert_eq!(build_match_query(""), None);
        assert_eq!(build_match_query("   "), None);
        assert_eq!(build_match_query("* () -"), None);
    }
}
