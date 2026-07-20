//! Running an advanced-search query against the local store.
//!
//! The runner narrows to a candidate set by scope (a single folder, a folder
//! subtree, or every folder), bulk-loads the facts those candidates need in a
//! handful of `IN (…)` queries (never N+1), evaluates the compiled
//! [`SearchProgram`](super::eval::SearchProgram) against each, then sorts and
//! truncates the matches. To keep memory and latency bounded on a very large
//! store it examines at most [`SCAN_CAP`] of the most recent messages in scope —
//! the same "local, bounded, most-recent-first" shape the full-text search box
//! uses (ADR-0008).

use std::collections::{HashMap, HashSet};

use axomail_core::{AccountId, MailError, MailboxId, MessageId};
use sqlx::SqlitePool;

use super::eval::{SearchMessageFacts, SearchProgram};
use super::query::{SearchQuery, SearchScope, SearchSort};
use crate::error_map::map_sqlx;
use crate::pagination::NULL_DATE_SORT;

/// Upper bound on the number of most-recent in-scope messages a single search
/// examines. A match older than this frontier in a huge folder is not found —
/// a documented, bounded-work trade-off (ADR-0008, local-first search).
const SCAN_CAP: i64 = 5_000;

/// The longest snippet (characters of the message preview) carried on a hit.
const SNIPPET_CHARS: usize = 140;

/// One advanced-search hit: which message matched, where it lives, and a short
/// plain-text preview snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// The matching message.
    pub message_id: MessageId,
    /// The folder it lives in.
    pub mailbox_id: MailboxId,
    /// The owning account.
    pub account: AccountId,
    /// A short plain-text preview of the message (no highlight markers).
    pub snippet: String,
}

/// Runs `query` against the store, returning up to `limit` matched messages in
/// the query's sort order.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub(super) async fn run_query(
    pool: &SqlitePool,
    query: &SearchQuery,
    limit: u32,
) -> Result<Vec<SearchMatch>, MailError> {
    let candidates = load_candidates(pool, &query.scope).await?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();

    let accounts = load_accounts(pool).await?;
    let recipients = load_recipients(pool, &ids).await?;
    let tags = load_tags(pool, &ids).await?;
    let bodies = load_bodies(pool, &ids).await?;
    let attachments = load_attachments(pool, &ids).await?;

    let program = SearchProgram::compile(query);
    let mut matched: Vec<CandidateRow> = candidates
        .into_iter()
        .filter(|row| {
            let facts = row.to_facts(&accounts, &recipients, &tags, &bodies, &attachments);
            program.matches(&facts)
        })
        .collect();

    sort_matches(&mut matched, query.sort);
    matched.truncate(limit.max(1) as usize);
    Ok(matched.into_iter().map(CandidateRow::into_match).collect())
}

/// Sorts matched candidate rows in place per the requested order.
fn sort_matches(rows: &mut [CandidateRow], sort: SearchSort) {
    match sort {
        // Nulls sort last in both directions (sentinel min/max on the date key).
        SearchSort::DateDesc => rows.sort_by(|a, b| {
            b.date()
                .unwrap_or(NULL_DATE_SORT)
                .cmp(&a.date().unwrap_or(NULL_DATE_SORT))
                .then(b.id.cmp(&a.id))
        }),
        SearchSort::DateAsc => rows.sort_by(|a, b| {
            a.date()
                .unwrap_or(i64::MAX)
                .cmp(&b.date().unwrap_or(i64::MAX))
                .then(a.id.cmp(&b.id))
        }),
        SearchSort::SubjectAsc => {
            rows.sort_by(|a, b| a.subject_key().cmp(&b.subject_key()).then(a.id.cmp(&b.id)))
        }
        SearchSort::SubjectDesc => {
            rows.sort_by(|a, b| b.subject_key().cmp(&a.subject_key()).then(b.id.cmp(&a.id)))
        }
    }
}

/// Loads the in-scope candidate rows, newest first, capped at [`SCAN_CAP`].
async fn load_candidates(
    pool: &SqlitePool,
    scope: &SearchScope,
) -> Result<Vec<CandidateRow>, MailError> {
    let select = "SELECT m.id AS id, m.mailbox_id AS mailbox_id, mb.account_id AS account_id, \
         m.subject AS subject, m.date_unix AS date_unix, m.preview AS preview, \
         m.flag_seen AS flag_seen, m.flag_flagged AS flag_flagged, mb.name AS mailbox_name \
         FROM messages m JOIN mailboxes mb ON mb.id = m.mailbox_id";
    let order = " ORDER BY COALESCE(m.date_unix, ?) DESC, m.id DESC LIMIT ?";

    let (sql, mailbox_bind) = match scope {
        SearchScope::AllFolders => (format!("{select}{order}"), None),
        SearchScope::Folder { mailbox_id } => (
            format!("{select} WHERE m.mailbox_id = ?{order}"),
            Some(mailbox_id.value()),
        ),
        SearchScope::FolderRecursive { mailbox_id } => (
            format!(
                "WITH RECURSIVE subtree(id) AS (\
                     SELECT ? UNION ALL \
                     SELECT mb2.id FROM mailboxes mb2 JOIN subtree s ON mb2.parent_id = s.id) \
                 {select} WHERE m.mailbox_id IN (SELECT id FROM subtree){order}"
            ),
            Some(mailbox_id.value()),
        ),
    };

    let mut q = sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(sql));
    // Bind in SQL-text order. For both scoped variants the mailbox `?` (the CTE
    // root, or the `mailbox_id = ?` predicate) precedes the `COALESCE(..., ?)`
    // and `LIMIT ?` binds; `AllFolders` has no mailbox bind at all.
    if let Some(id) = mailbox_bind {
        q = q.bind(id);
    }
    q = q.bind(NULL_DATE_SORT).bind(SCAN_CAP);

    q.fetch_all(pool)
        .await
        .map_err(|error| map_sqlx("saved_search_candidates", error))
}

/// Loads every account's lowercased "display name + email" text, keyed by id.
async fn load_accounts(pool: &SqlitePool) -> Result<HashMap<i64, String>, MailError> {
    let rows =
        sqlx::query_as::<_, (i64, String, String)>("SELECT id, display_name, email FROM accounts")
            .fetch_all(pool)
            .await
            .map_err(|error| map_sqlx("saved_search_accounts", error))?;
    Ok(rows
        .into_iter()
        .map(|(id, name, email)| (id, format!("{name} {email}").to_lowercase()))
        .collect())
}

/// Loads from/to/cc addresses (lowercased `name email`), keyed by message id.
async fn load_recipients(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<HashMap<i64, Recipients>, MailError> {
    let sql = format!(
        "SELECT message_id, kind, display_name, email FROM message_recipients \
         WHERE message_id IN ({}) ORDER BY message_id, kind, position, id",
        placeholders(ids.len())
    );
    let rows = bind_ids(
        sqlx::query_as::<_, RecipientRow>(sqlx::AssertSqlSafe(sql)),
        ids,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| map_sqlx("saved_search_recipients", error))?;
    let mut map: HashMap<i64, Recipients> = HashMap::new();
    for row in rows {
        let rendered = render_address(row.display_name.as_deref(), &row.email);
        let entry = map.entry(row.message_id).or_default();
        match row.kind.as_str() {
            "from" => entry.from.push(rendered),
            "to" => entry.to.push(rendered),
            "cc" => entry.cc.push(rendered),
            _ => {}
        }
    }
    Ok(map)
}

/// Loads each message's tag texts (keyword + display name, lowercased).
async fn load_tags(pool: &SqlitePool, ids: &[i64]) -> Result<HashMap<i64, Vec<String>>, MailError> {
    let sql = format!(
        "SELECT mt.message_id, t.keyword, t.display_name FROM message_tags mt \
         JOIN tags t ON t.id = mt.tag_id WHERE mt.message_id IN ({})",
        placeholders(ids.len())
    );
    let rows = bind_ids(
        sqlx::query_as::<_, (i64, String, String)>(sqlx::AssertSqlSafe(sql)),
        ids,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| map_sqlx("saved_search_tags", error))?;
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    for (message_id, keyword, display) in rows {
        let entry = map.entry(message_id).or_default();
        entry.push(keyword.to_lowercase());
        entry.push(display.to_lowercase());
    }
    Ok(map)
}

/// Loads each message's indexed body text, keyed by message id.
async fn load_bodies(pool: &SqlitePool, ids: &[i64]) -> Result<HashMap<i64, String>, MailError> {
    let sql = format!(
        "SELECT message_id, body FROM message_search_content WHERE message_id IN ({})",
        placeholders(ids.len())
    );
    let rows = bind_ids(
        sqlx::query_as::<_, (i64, String)>(sqlx::AssertSqlSafe(sql)),
        ids,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| map_sqlx("saved_search_bodies", error))?;
    Ok(rows.into_iter().collect())
}

/// Loads the set of message ids that have at least one attachment.
async fn load_attachments(pool: &SqlitePool, ids: &[i64]) -> Result<HashSet<i64>, MailError> {
    let sql = format!(
        "SELECT DISTINCT message_id FROM attachments WHERE message_id IN ({})",
        placeholders(ids.len())
    );
    let rows = bind_ids(sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql)), ids)
        .fetch_all(pool)
        .await
        .map_err(|error| map_sqlx("saved_search_attachments", error))?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// A `?, ?, …` list of `n` placeholders (at least one).
fn placeholders(n: usize) -> String {
    vec!["?"; n.max(1)].join(", ")
}

/// Binds a slice of ids onto an `IN (…)` query in order.
fn bind_ids<'q, T>(
    mut query: sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments>,
    ids: &'q [i64],
) -> sqlx::query::QueryAs<'q, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments> {
    for id in ids {
        query = query.bind(*id);
    }
    query
}

/// Renders an address as a lowercased `name email` (or just the email).
fn render_address(display_name: Option<&str>, email: &str) -> String {
    match display_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("{name} {email}").to_lowercase(),
        None => email.to_lowercase(),
    }
}

/// Per-message address buckets used while building facts.
#[derive(Default)]
struct Recipients {
    from: Vec<String>,
    to: Vec<String>,
    cc: Vec<String>,
}

/// One `message_recipients` row.
#[derive(sqlx::FromRow)]
struct RecipientRow {
    message_id: i64,
    kind: String,
    display_name: Option<String>,
    email: String,
}

/// One in-scope candidate message row.
#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: i64,
    mailbox_id: i64,
    account_id: i64,
    subject: Option<String>,
    date_unix: Option<i64>,
    preview: Option<String>,
    flag_seen: i64,
    flag_flagged: i64,
    mailbox_name: String,
}

impl CandidateRow {
    /// The date used for date-order sorting.
    fn date(&self) -> Option<i64> {
        self.date_unix
    }

    /// The lowercased subject key used for subject sorting.
    fn subject_key(&self) -> String {
        self.subject.as_deref().unwrap_or_default().to_lowercase()
    }

    /// Assembles the evaluator facts from the bulk-loaded maps.
    fn to_facts(
        &self,
        accounts: &HashMap<i64, String>,
        recipients: &HashMap<i64, Recipients>,
        tags: &HashMap<i64, Vec<String>>,
        bodies: &HashMap<i64, String>,
        attachments: &HashSet<i64>,
    ) -> SearchMessageFacts {
        let addr = recipients.get(&self.id);
        let body = bodies
            .get(&self.id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| self.preview.clone())
            .unwrap_or_default()
            .to_lowercase();
        SearchMessageFacts {
            subject: self.subject.clone().unwrap_or_default().to_lowercase(),
            from: addr.map(|a| a.from.clone()).unwrap_or_default(),
            to: addr.map(|a| a.to.clone()).unwrap_or_default(),
            cc: addr.map(|a| a.cc.clone()).unwrap_or_default(),
            body,
            tags: tags.get(&self.id).cloned().unwrap_or_default(),
            folder: self.mailbox_name.to_lowercase(),
            account: accounts.get(&self.account_id).cloned().unwrap_or_default(),
            has_attachment: attachments.contains(&self.id),
            read: self.flag_seen != 0,
            starred: self.flag_flagged != 0,
            date: self.date_unix,
        }
    }

    /// Builds the output hit, carrying a short plain-text preview snippet.
    fn into_match(self) -> SearchMatch {
        let snippet = self
            .preview
            .as_deref()
            .map(|text| {
                let trimmed = text.trim();
                trimmed.chars().take(SNIPPET_CHARS).collect::<String>()
            })
            .unwrap_or_default();
        SearchMatch {
            message_id: MessageId::new(self.id),
            mailbox_id: MailboxId::new(self.mailbox_id),
            account: AccountId::new(self.account_id),
            snippet,
        }
    }
}
