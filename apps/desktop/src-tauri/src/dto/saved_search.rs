//! DTOs for the advanced-search / saved-search surface.
//!
//! The condition field/op and the match-type/sort enums are the storage enums
//! re-used directly (they serialize to their snake_case string forms, which the
//! frontend mirrors), exactly as the filter DTOs do. The scope is a small tagged
//! shape (`{ kind, mailboxId }`) that maps onto [`SearchScope`]. Every id
//! crossing the IPC boundary is routed through [`checked_id`](super::checked_id).

use axomail_core::{MailError, MailboxId};
use axomail_storage::{
    ConditionOp, MatchType, SavedSearch, SavedSearchInput, SearchCondition, SearchField,
    SearchQuery, SearchScope, SearchSort,
};
use serde::{Deserialize, Serialize};

use super::checked_id;

/// One advanced-search condition as exchanged with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchConditionDto {
    /// Which message part the condition tests.
    pub field: SearchField,
    /// How it is compared (reused from the filter model).
    pub op: ConditionOp,
    /// The operand text.
    pub value: String,
}

impl SearchConditionDto {
    fn into_condition(self) -> SearchCondition {
        SearchCondition {
            field: self.field,
            op: self.op,
            value: self.value,
        }
    }

    fn from_condition(condition: &SearchCondition) -> Self {
        Self {
            field: condition.field,
            op: condition.op,
            value: condition.value.clone(),
        }
    }
}

/// The structural scope kind, mirroring [`SearchScope`]'s variants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScopeKind {
    /// Every folder of every account.
    AllFolders,
    /// A single folder.
    Folder,
    /// A folder and all of its descendants.
    FolderRecursive,
}

/// The search scope as exchanged with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchScopeDto {
    /// Which kind of scope this is.
    pub kind: SearchScopeKind,
    /// The target folder for the folder / folder-recursive kinds.
    pub mailbox_id: Option<i64>,
}

impl SearchScopeDto {
    fn into_scope(self) -> Result<SearchScope, MailError> {
        let mailbox = || {
            self.mailbox_id
                .map(MailboxId::new)
                .ok_or_else(|| MailError::Configuration {
                    message: "a folder-scoped search needs a folder".to_owned(),
                })
        };
        Ok(match self.kind {
            SearchScopeKind::AllFolders => SearchScope::AllFolders,
            SearchScopeKind::Folder => SearchScope::Folder {
                mailbox_id: mailbox()?,
            },
            SearchScopeKind::FolderRecursive => SearchScope::FolderRecursive {
                mailbox_id: mailbox()?,
            },
        })
    }

    fn from_scope(scope: &SearchScope) -> Result<Self, MailError> {
        Ok(match scope {
            SearchScope::AllFolders => Self {
                kind: SearchScopeKind::AllFolders,
                mailbox_id: None,
            },
            SearchScope::Folder { mailbox_id } => Self {
                kind: SearchScopeKind::Folder,
                mailbox_id: Some(checked_id(mailbox_id.value())?),
            },
            SearchScope::FolderRecursive { mailbox_id } => Self {
                kind: SearchScopeKind::FolderRecursive,
                mailbox_id: Some(checked_id(mailbox_id.value())?),
            },
        })
    }
}

/// A complete advanced-search query as exchanged with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQueryDto {
    /// How the conditions combine.
    pub match_type: MatchType,
    /// The conditions, in order.
    pub conditions: Vec<SearchConditionDto>,
    /// The structural scope.
    pub scope: SearchScopeDto,
    /// The result ordering.
    pub sort: SearchSort,
}

impl SearchQueryDto {
    /// Converts the payload into the storage query type.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a folder scope has no folder.
    pub fn into_query(self) -> Result<SearchQuery, MailError> {
        Ok(SearchQuery {
            match_type: self.match_type,
            conditions: self
                .conditions
                .into_iter()
                .map(SearchConditionDto::into_condition)
                .collect(),
            scope: self.scope.into_scope()?,
            sort: self.sort,
        })
    }

    /// Builds the DTO from a stored query.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a scoped folder id is out of range.
    pub fn from_query(query: &SearchQuery) -> Result<Self, MailError> {
        Ok(Self {
            match_type: query.match_type,
            conditions: query
                .conditions
                .iter()
                .map(SearchConditionDto::from_condition)
                .collect(),
            scope: SearchScopeDto::from_scope(&query.scope)?,
            sort: query.sort,
        })
    }
}

/// A persisted saved search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearchDto {
    /// Row identifier.
    pub id: i64,
    /// User-visible name.
    pub name: String,
    /// The query it runs.
    pub query: SearchQueryDto,
}

impl SavedSearchDto {
    /// Builds the DTO from a stored saved search, checking every id.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if an id is out of safe-integer range.
    pub fn from_saved(saved: &SavedSearch) -> Result<Self, MailError> {
        Ok(Self {
            id: checked_id(saved.id.value())?,
            name: saved.name.clone(),
            query: SearchQueryDto::from_query(&saved.query)?,
        })
    }
}

/// The create/update payload for a saved search.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearchInputDto {
    /// User-visible name.
    pub name: String,
    /// The query to run.
    pub query: SearchQueryDto,
}

impl SavedSearchInputDto {
    /// Converts the payload into the storage input type.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a folder scope has no folder.
    pub fn into_input(self) -> Result<SavedSearchInput, MailError> {
        Ok(SavedSearchInput {
            name: self.name.trim().to_owned(),
            query: self.query.into_query()?,
        })
    }
}
