//! Filter repository: message rules (conditions -> actions) and their
//! run-on-arrival bookkeeping.
//!
//! A [`Filter`] owns ordered child [`FilterCondition`]s and [`FilterAction`]s
//! (both cascade on delete). CRUD writes the parent and children in one
//! transaction; reads batch the children for a page of filters to avoid an
//! N+1. `account = None` is a *global* rule that runs on every account;
//! [`FilterRepo::active_for_account`] returns an account's own enabled rules
//! followed by the enabled global rules, in evaluation order.
//!
//! The pure evaluator ([`eval`]) and the DB facts loader ([`facts`]) are
//! siblings; arrival-pass state (the per-account toggle and high-water mark)
//! lives in [`arrival`].

mod arrival;
mod eval;
mod facts;
mod rows;
mod types;

use axomail_core::{AccountId, MailError};
use sqlx::SqlitePool;

pub use arrival::ArrivalState;
pub use eval::{FilterMessageFacts, FilterProgram};
pub use types::{
    ActionKind, ConditionField, ConditionOp, Filter, FilterAction, FilterCondition, FilterId,
    FilterInput, MatchType,
};

use crate::error_map::map_sqlx;
use rows::{ActionRow, ConditionRow, FilterRow, bind_ids, insert_children, next_sort_order};

/// Validates a filter's content before it is stored: a non-blank name, at least
/// one condition and one action, and a compilable pattern for every
/// `matches_regex` condition (rejecting a bad regex here, rather than silently
/// never matching it at run time).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] describing the first problem found.
pub fn validate_filter_input(input: &FilterInput) -> Result<(), MailError> {
    let config = |message: &str| MailError::Configuration {
        message: message.to_owned(),
    };
    if input.name.trim().is_empty() {
        return Err(config("a filter needs a name"));
    }
    if input.conditions.is_empty() {
        return Err(config("a filter needs at least one condition"));
    }
    if input.actions.is_empty() {
        return Err(config("a filter needs at least one action"));
    }
    for condition in &input.conditions {
        if condition.op == ConditionOp::MatchesRegex && regex::Regex::new(&condition.value).is_err()
        {
            return Err(config(
                "a regular-expression condition has an invalid pattern",
            ));
        }
    }
    Ok(())
}

/// Parent columns selected when reading a filter, in [`FilterRow`] order.
const FILTER_COLUMNS: &str = "id, account_id, name, enabled, match_type, sort_order, stop_on_match";

/// Repository over the `filters`, `filter_conditions`, `filter_actions`, and
/// `filter_arrival_state` tables.
#[derive(Debug)]
pub struct FilterRepo<'a> {
    // Crate-visible so the sibling `facts` and `arrival` modules can query.
    pub(crate) pool: &'a SqlitePool,
}

impl<'a> FilterRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Creates a filter (appended to the end of its scope's rule list) with its
    /// conditions and actions, returning the persisted filter.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn create(&self, input: &FilterInput) -> Result<Filter, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("create_filter", error))?;

        let sort_order = next_sort_order(&mut tx, input.account).await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO filters (account_id, name, enabled, match_type, sort_order, stop_on_match)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(input.account.map(AccountId::value))
        .bind(input.name.as_str())
        .bind(input.enabled)
        .bind(input.match_type.as_str())
        .bind(sort_order)
        .bind(input.stop_on_match)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| map_sqlx("create_filter", error))?;

        insert_children(&mut tx, id, input).await?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("create_filter", error))?;

        Ok(Filter {
            id: FilterId::new(id),
            account: input.account,
            name: input.name.clone(),
            enabled: input.enabled,
            match_type: input.match_type,
            sort_order,
            stop_on_match: input.stop_on_match,
            conditions: input.conditions.clone(),
            actions: input.actions.clone(),
        })
    }

    /// Replaces a filter's fields, conditions, and actions (its `sort_order` is
    /// preserved — reordering is a separate call). Returns the updated filter,
    /// or `None` if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn update(
        &self,
        id: FilterId,
        input: &FilterInput,
    ) -> Result<Option<Filter>, MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("update_filter", error))?;

        let affected = sqlx::query(
            "UPDATE filters SET account_id = ?, name = ?, enabled = ?, match_type = ?, \
             stop_on_match = ? WHERE id = ?",
        )
        .bind(input.account.map(AccountId::value))
        .bind(input.name.as_str())
        .bind(input.enabled)
        .bind(input.match_type.as_str())
        .bind(input.stop_on_match)
        .bind(id.value())
        .execute(&mut *tx)
        .await
        .map_err(|error| map_sqlx("update_filter", error))?
        .rows_affected();
        if affected == 0 {
            return Ok(None);
        }

        sqlx::query("DELETE FROM filter_conditions WHERE filter_id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("update_filter", error))?;
        sqlx::query("DELETE FROM filter_actions WHERE filter_id = ?")
            .bind(id.value())
            .execute(&mut *tx)
            .await
            .map_err(|error| map_sqlx("update_filter", error))?;
        insert_children(&mut tx, id.value(), input).await?;
        tx.commit()
            .await
            .map_err(|error| map_sqlx("update_filter", error))?;

        self.get(id).await
    }

    /// Toggles a filter's `enabled` flag without rewriting its children.
    /// Returns whether a row changed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn set_enabled(&self, id: FilterId, enabled: bool) -> Result<bool, MailError> {
        let affected = sqlx::query("UPDATE filters SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("set_filter_enabled", error))?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Deletes a filter (cascading to its conditions and actions). Returns
    /// whether a row was removed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn delete(&self, id: FilterId) -> Result<bool, MailError> {
        let affected = sqlx::query("DELETE FROM filters WHERE id = ?")
            .bind(id.value())
            .execute(self.pool)
            .await
            .map_err(|error| map_sqlx("delete_filter", error))?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Fetches a single filter with its children, or `None` if absent.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get(&self, id: FilterId) -> Result<Option<Filter>, MailError> {
        let row = sqlx::query_as::<_, FilterRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {FILTER_COLUMNS} FROM filters WHERE id = ?"
        )))
        .bind(id.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_filter", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(
            self.assemble(vec![row])
                .await?
                .into_iter()
                .next()
                .ok_or(MailError::Database {
                    message: "get_filter: assembled zero filters".to_owned(),
                })?,
        ))
    }

    /// Lists the filters in a single scope (a specific account, or the global
    /// rules when `account` is `None`) in rule-list order.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn list(&self, account: Option<AccountId>) -> Result<Vec<Filter>, MailError> {
        let predicate = match account {
            Some(_) => "account_id = ?",
            None => "account_id IS NULL",
        };
        let mut query = sqlx::query_as::<_, FilterRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {FILTER_COLUMNS} FROM filters WHERE {predicate} ORDER BY sort_order, id"
        )));
        if let Some(account) = account {
            query = query.bind(account.value());
        }
        let rows = query
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("list_filters", error))?;
        self.assemble(rows).await
    }

    /// Returns the enabled filters that apply to `account`, in evaluation order:
    /// the account's own rules first, then the global (all-account) rules.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn active_for_account(&self, account: AccountId) -> Result<Vec<Filter>, MailError> {
        let rows = sqlx::query_as::<_, FilterRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {FILTER_COLUMNS} FROM filters
             WHERE enabled = 1 AND (account_id = ? OR account_id IS NULL)
             ORDER BY (account_id IS NULL), sort_order, id"
        )))
        .bind(account.value())
        .fetch_all(self.pool)
        .await
        .map_err(|error| map_sqlx("active_filters", error))?;
        self.assemble(rows).await
    }

    /// Sets the `sort_order` of the given filters to their position in
    /// `ordered_ids`, scoped to filters that belong to `account`. Ids not in the
    /// scope are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn reorder(
        &self,
        account: Option<AccountId>,
        ordered_ids: &[FilterId],
    ) -> Result<(), MailError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| map_sqlx("reorder_filters", error))?;
        let predicate = match account {
            Some(_) => "account_id = ?",
            None => "account_id IS NULL",
        };
        for (position, id) in ordered_ids.iter().enumerate() {
            let sql = format!("UPDATE filters SET sort_order = ? WHERE id = ? AND {predicate}");
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(i64::try_from(position).unwrap_or(i64::MAX))
                .bind(id.value());
            if let Some(account) = account {
                query = query.bind(account.value());
            }
            query
                .execute(&mut *tx)
                .await
                .map_err(|error| map_sqlx("reorder_filters", error))?;
        }
        tx.commit()
            .await
            .map_err(|error| map_sqlx("reorder_filters", error))
    }

    /// Loads and attaches the conditions and actions for a set of parent rows.
    async fn assemble(&self, rows: Vec<FilterRow>) -> Result<Vec<Filter>, MailError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let placeholders = vec!["?"; ids.len()].join(", ");

        let conditions = sqlx::query_as::<_, ConditionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT filter_id, field, op, value FROM filter_conditions
             WHERE filter_id IN ({placeholders}) ORDER BY filter_id, position, id"
        )));
        let conditions = bind_ids(conditions, &ids)
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("load_filter_conditions", error))?;

        let actions = sqlx::query_as::<_, ActionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT filter_id, kind, param FROM filter_actions
             WHERE filter_id IN ({placeholders}) ORDER BY filter_id, position, id"
        )));
        let actions = bind_ids(actions, &ids)
            .fetch_all(self.pool)
            .await
            .map_err(|error| map_sqlx("load_filter_actions", error))?;

        let mut filters = Vec::with_capacity(rows.len());
        for row in rows {
            let mut filter = row.into_filter()?;
            for condition in conditions
                .iter()
                .filter(|c| c.filter_id == filter.id.value())
            {
                filter.conditions.push(condition.to_condition()?);
            }
            for action in actions.iter().filter(|a| a.filter_id == filter.id.value()) {
                filter.actions.push(action.to_action()?);
            }
            filters.push(filter);
        }
        Ok(filters)
    }
}
