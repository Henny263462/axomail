//! Row structs and write helpers for the filter repository, kept in a sibling
//! module so `filters.rs` stays under the file-size gate.

use axomail_core::{AccountId, MailError};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use super::types::{
    ActionKind, ConditionField, ConditionOp, Filter, FilterAction, FilterCondition, FilterId,
    FilterInput, MatchType,
};
use crate::error_map::map_sqlx;

/// The next `sort_order` (max + 1) within a scope.
pub(super) async fn next_sort_order(
    tx: &mut Transaction<'_, Sqlite>,
    account: Option<AccountId>,
) -> Result<i64, MailError> {
    let predicate = match account {
        Some(_) => "account_id = ?",
        None => "account_id IS NULL",
    };
    let sql = format!("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM filters WHERE {predicate}");
    let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
    if let Some(account) = account {
        query = query.bind(account.value());
    }
    query
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| map_sqlx("next_filter_sort_order", error))
}

/// Inserts a filter's conditions and actions, preserving position order.
pub(super) async fn insert_children(
    conn: &mut SqliteConnection,
    filter_id: i64,
    input: &FilterInput,
) -> Result<(), MailError> {
    for (position, condition) in input.conditions.iter().enumerate() {
        sqlx::query(
            "INSERT INTO filter_conditions (filter_id, field, op, value, position)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(filter_id)
        .bind(condition.field.as_str())
        .bind(condition.op.as_str())
        .bind(condition.value.as_str())
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("insert_filter_condition", error))?;
    }
    for (position, action) in input.actions.iter().enumerate() {
        sqlx::query(
            "INSERT INTO filter_actions (filter_id, kind, param, position)
             VALUES (?, ?, ?, ?)",
        )
        .bind(filter_id)
        .bind(action.kind.as_str())
        .bind(action.param.as_str())
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await
        .map_err(|error| map_sqlx("insert_filter_action", error))?;
    }
    Ok(())
}

/// Binds a slice of ids onto an `IN (...)` query in order.
pub(super) fn bind_ids<'q, T>(
    mut query: sqlx::query::QueryAs<'q, Sqlite, T, sqlx::sqlite::SqliteArguments>,
    ids: &'q [i64],
) -> sqlx::query::QueryAs<'q, Sqlite, T, sqlx::sqlite::SqliteArguments> {
    for id in ids {
        query = query.bind(*id);
    }
    query
}

/// Raw `filters` parent row.
#[derive(sqlx::FromRow)]
pub(super) struct FilterRow {
    pub id: i64,
    pub account_id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub match_type: String,
    pub sort_order: i64,
    pub stop_on_match: bool,
}

impl FilterRow {
    pub(super) fn into_filter(self) -> Result<Filter, MailError> {
        Ok(Filter {
            id: FilterId::new(self.id),
            account: self.account_id.map(AccountId::new),
            name: self.name,
            enabled: self.enabled,
            match_type: MatchType::parse(&self.match_type)?,
            sort_order: self.sort_order,
            stop_on_match: self.stop_on_match,
            conditions: Vec::new(),
            actions: Vec::new(),
        })
    }
}

/// Raw `filter_conditions` row.
#[derive(sqlx::FromRow)]
pub(super) struct ConditionRow {
    pub filter_id: i64,
    pub field: String,
    pub op: String,
    pub value: String,
}

impl ConditionRow {
    pub(super) fn to_condition(&self) -> Result<FilterCondition, MailError> {
        Ok(FilterCondition {
            field: ConditionField::parse(&self.field)?,
            op: ConditionOp::parse(&self.op)?,
            value: self.value.clone(),
        })
    }
}

/// Raw `filter_actions` row.
#[derive(sqlx::FromRow)]
pub(super) struct ActionRow {
    pub filter_id: i64,
    pub kind: String,
    pub param: String,
}

impl ActionRow {
    pub(super) fn to_action(&self) -> Result<FilterAction, MailError> {
        Ok(FilterAction {
            kind: ActionKind::parse(&self.kind)?,
            param: self.param.clone(),
        })
    }
}
