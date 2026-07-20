//! DTOs for the message-filter (rules engine) surface.
//!
//! The condition/action enums are the storage enums re-used directly: they
//! serialize to their snake_case string form (`"from"`, `"matches_regex"`,
//! `"delete_to_trash"`), which the frontend mirrors. Every id crossing the IPC
//! boundary is routed through [`checked_id`](super::checked_id).

use axomail_core::{AccountId, MailError};
use axomail_storage::{
    ActionKind, ConditionField, ConditionOp, Filter, FilterAction, FilterCondition, FilterInput,
    MatchType,
};
use axomail_sync::FilterRunReport;
use serde::{Deserialize, Serialize};

use super::checked_id;

/// One filter condition as exchanged with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterConditionDto {
    /// Which message part the condition tests.
    pub field: ConditionField,
    /// How it is compared.
    pub op: ConditionOp,
    /// The operand text.
    pub value: String,
}

/// One filter action as exchanged with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterActionDto {
    /// What the action does.
    pub kind: ActionKind,
    /// The action operand (may be empty).
    pub param: String,
}

/// A persisted filter with its conditions and actions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterDto {
    /// Row identifier.
    pub id: i64,
    /// Owning account, or `None` for a global (all-accounts) rule.
    pub account_id: Option<i64>,
    /// User-visible name.
    pub name: String,
    /// Whether the rule is active.
    pub enabled: bool,
    /// How the conditions combine.
    pub match_type: MatchType,
    /// Ordering within the rule list.
    pub sort_order: i64,
    /// Whether a match halts evaluation of later filters.
    pub stop_on_match: bool,
    /// Conditions in order.
    pub conditions: Vec<FilterConditionDto>,
    /// Actions in order.
    pub actions: Vec<FilterActionDto>,
}

impl FilterDto {
    /// Builds the DTO from a stored filter, checking every id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if an id is out of safe-integer range.
    pub fn from_filter(filter: &Filter) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(filter.id.value())?,
            account_id: filter.account.map(|a| checked_id(a.value())).transpose()?,
            name: filter.name.clone(),
            enabled: filter.enabled,
            match_type: filter.match_type,
            sort_order: filter.sort_order,
            stop_on_match: filter.stop_on_match,
            conditions: filter
                .conditions
                .iter()
                .map(|c| FilterConditionDto {
                    field: c.field,
                    op: c.op,
                    value: c.value.clone(),
                })
                .collect(),
            actions: filter
                .actions
                .iter()
                .map(|a| FilterActionDto {
                    kind: a.kind,
                    param: a.param.clone(),
                })
                .collect(),
        })
    }
}

/// The create/update payload for a filter.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterInputDto {
    /// Owning account, or `None` for a global rule.
    pub account_id: Option<i64>,
    /// User-visible name.
    pub name: String,
    /// Whether the rule is active.
    pub enabled: bool,
    /// How the conditions combine.
    pub match_type: MatchType,
    /// Whether a match halts evaluation of later filters.
    pub stop_on_match: bool,
    /// Conditions in order.
    pub conditions: Vec<FilterConditionDto>,
    /// Actions in order.
    pub actions: Vec<FilterActionDto>,
}

impl FilterInputDto {
    /// Converts the payload into the storage input type.
    #[must_use]
    pub fn into_input(self) -> FilterInput {
        FilterInput {
            account: self.account_id.map(AccountId::new),
            name: self.name.trim().to_owned(),
            enabled: self.enabled,
            match_type: self.match_type,
            stop_on_match: self.stop_on_match,
            conditions: self
                .conditions
                .into_iter()
                .map(|c| FilterCondition {
                    field: c.field,
                    op: c.op,
                    value: c.value,
                })
                .collect(),
            actions: self
                .actions
                .into_iter()
                .map(|a| FilterAction {
                    kind: a.kind,
                    param: a.param.trim().to_owned(),
                })
                .collect(),
        }
    }
}

/// The result of a manual filter run over a folder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRunOutcomeDto {
    /// Messages that matched at least one filter.
    pub matched: u64,
    /// Actions that produced an effect.
    pub actions_applied: u64,
    /// Forwards queued to the outbox.
    pub forwards_queued: u64,
}

impl From<FilterRunReport> for FilterRunOutcomeDto {
    fn from(report: FilterRunReport) -> Self {
        Self {
            matched: report.matched,
            actions_applied: report.actions_applied,
            forwards_queued: report.forwards_queued,
        }
    }
}

/// An account's run-on-arrival filtering setting.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFilterSettingsDto {
    /// Whether filters run automatically on newly-arrived inbox mail.
    pub arrival_enabled: bool,
}
