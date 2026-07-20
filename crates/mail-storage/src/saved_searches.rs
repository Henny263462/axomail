//! Saved-search repository: named, reusable advanced queries surfaced as
//! sidebar virtual folders.
//!
//! A [`SavedSearch`] persists a [`SearchQuery`] (its condition set, scope, and
//! sort) split across the `saved_searches` columns (migration 0022). The same
//! [`SearchQuery`] type is what the ad-hoc advanced-search builder runs, so a
//! preview and a saved search take exactly the same evaluation path
//! ([`run`](SavedSearchRepo::run)).
//!
//! The query model ([`query`]), the pure evaluator ([`eval`]), and the DB runner
//! ([`run`]) are siblings; this root owns CRUD and the two run entry points.

mod eval;
mod query;
mod run;

use axomail_core::MailError;
use sqlx::SqlitePool;

pub use query::{SearchCondition, SearchField, SearchQuery, SearchScope, SearchSort};
pub use run::SearchMatch;

use crate::error_map::map_sqlx;
use crate::filters::ConditionOp;

/// Validates a query before it is run or persisted: every `matches_regex`
/// condition must carry a compilable pattern (rejecting a bad regex here rather
/// than letting it silently never match at run time).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] describing the first bad pattern.
pub fn validate_search_query(query: &SearchQuery) -> Result<(), MailError> {
    for condition in &query.conditions {
        if condition.op == ConditionOp::MatchesRegex && regex::Regex::new(&condition.value).is_err()
        {
            return Err(MailError::Configuration {
                message: "a regular-expression condition has an invalid pattern".to_owned(),
            });
        }
    }
    Ok(())
}

/// Validates a saved search's content: a non-blank name and a valid query.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] describing the first problem found.
pub fn validate_saved_search_input(input: &SavedSearchInput) -> Result<(), MailError> {
    if input.name.trim().is_empty() {
        return Err(MailError::Configuration {
            message: "a saved search needs a name".to_owned(),
        });
    }
    validate_search_query(&input.query)
}

/// Identifier of a persisted [`SavedSearch`] row.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct SavedSearchId(i64);

impl SavedSearchId {
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

impl std::fmt::Display for SavedSearchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A persisted saved search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSearch {
    /// Row identifier.
    pub id: SavedSearchId,
    /// User-visible name.
    pub name: String,
    /// The query it runs.
    pub query: SearchQuery,
}

/// The editable content of a saved search (create and update).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSearchInput {
    /// User-visible name.
    pub name: String,
    /// The query to run.
    pub query: SearchQuery,
}

/// Repository over the `saved_searches` table.
#[derive(Debug)]
pub struct SavedSearchRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SavedSearchRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates a saved search and returns it.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create(&self, input: &SavedSearchInput) -> Result<SavedSearch, MailError> {
        let (query_json, scope, sort) = input.query.to_columns()?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO saved_searches (name, query_json, scope, sort) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(input.name.as_str())
        .bind(query_json)
        .bind(scope)
        .bind(sort)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("create_saved_search", error))?;
        Ok(SavedSearch {
            id: SavedSearchId::new(id),
            name: input.name.clone(),
            query: input.query.clone(),
        })
    }

    /// Replaces a saved search's name and query. Returns the updated record, or
    /// `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update(
        &self,
        id: SavedSearchId,
        input: &SavedSearchInput,
    ) -> Result<Option<SavedSearch>, MailError> {
        let (query_json, scope, sort) = input.query.to_columns()?;
        let affected = sqlx::query(
            "UPDATE saved_searches SET name = ?, query_json = ?, scope = ?, sort = ? WHERE id = ?",
        )
        .bind(input.name.as_str())
        .bind(query_json)
        .bind(scope)
        .bind(sort)
        .bind(id.value())
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("update_saved_search", error))?
        .rows_affected();
        if affected == 0 {
            return Ok(None);
        }
        Ok(Some(SavedSearch {
            id,
            name: input.name.clone(),
            query: input.query.clone(),
        }))
    }

    /// Deletes a saved search. Returns whether a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: SavedSearchId) -> Result<bool, MailError> {
        let affected = sqlx::query("DELETE FROM saved_searches WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_saved_search", error))?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Fetches a single saved search, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get(&self, id: SavedSearchId) -> Result<Option<SavedSearch>, MailError> {
        let row = sqlx::query_as::<_, SavedSearchRow>(
            "SELECT id, name, query_json, scope, sort FROM saved_searches WHERE id = ?",
        )
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_saved_search", error))?;
        row.map(SavedSearchRow::into_saved_search).transpose()
    }

    /// Lists all saved searches in alphabetical (case-insensitive) order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list(&self) -> Result<Vec<SavedSearch>, MailError> {
        let rows = sqlx::query_as::<_, SavedSearchRow>(
            "SELECT id, name, query_json, scope, sort FROM saved_searches \
             ORDER BY name COLLATE NOCASE, id",
        )
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("list_saved_searches", error))?;
        rows.into_iter()
            .map(SavedSearchRow::into_saved_search)
            .collect()
    }

    /// Runs an ad-hoc query, returning up to `limit` matched messages.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn run(
        &self,
        query: &SearchQuery,
        limit: u32,
    ) -> Result<Vec<SearchMatch>, MailError> {
        run::run_query(self.pool, query, limit).await
    }

    /// Runs a saved search by id, returning up to `limit` matched messages, or
    /// `None` if the saved search does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn run_saved(
        &self,
        id: SavedSearchId,
        limit: u32,
    ) -> Result<Option<Vec<SearchMatch>>, MailError> {
        let Some(saved) = self.get(id).await? else {
            return Ok(None);
        };
        Ok(Some(self.run(&saved.query, limit).await?))
    }
}

/// Raw `saved_searches` row.
#[derive(sqlx::FromRow)]
struct SavedSearchRow {
    id: i64,
    name: String,
    query_json: String,
    scope: String,
    sort: String,
}

impl SavedSearchRow {
    fn into_saved_search(self) -> Result<SavedSearch, MailError> {
        Ok(SavedSearch {
            id: SavedSearchId::new(self.id),
            name: self.name,
            query: SearchQuery::from_columns(&self.query_json, &self.scope, &self.sort)?,
        })
    }
}
