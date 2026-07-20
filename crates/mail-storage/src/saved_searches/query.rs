//! The advanced-search query model.
//!
//! A [`SearchQuery`] is the shared representation the advanced-search builder and
//! a persisted saved search both use. It deliberately reuses the Wave-4 filter
//! model's [`ConditionOp`](crate::filters::ConditionOp) and
//! [`MatchType`](crate::filters::MatchType) so the two subsystems speak one
//! condition language; it extends the *field* set with the facets a search needs
//! but a filter does not (folder / account / read / starred), and it carries a
//! structural [`SearchScope`] and a result [`SearchSort`] the filter model has no
//! use for.
//!
//! Persistence splits a query across the `saved_searches` columns: the condition
//! set (`match_type` + `conditions`) serializes to `query_json`, the scope to the
//! `scope` column, and the sort to the `sort` column (see the 0022 migration).

use axomail_core::{MailError, MailboxId};
use serde::{Deserialize, Serialize};

pub use crate::filters::{ConditionOp, MatchType};

/// Which part of a message an advanced-search condition tests.
///
/// A superset of the filter model's field set: `from`/`to`/`cc`/`subject`/
/// `body`/`date`/`tag`/`has_attachment` behave exactly as in a filter, while
/// `folder`/`account`/`read`/`starred` are search-only facets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchField {
    /// The `From` address(es).
    From,
    /// The `To` address(es).
    To,
    /// The `Cc` address(es).
    Cc,
    /// The subject line.
    Subject,
    /// The locally indexed plain-text body (preview until fully downloaded).
    Body,
    /// The message date (Unix seconds, or a `YYYY-MM-DD` operand).
    Date,
    /// A tag / IMAP keyword on the message.
    Tag,
    /// Whether the message carries any attachment.
    HasAttachment,
    /// The name of the folder the message lives in.
    Folder,
    /// The owning account's display name and email address.
    Account,
    /// Whether the message is read (the `\Seen` flag).
    Read,
    /// Whether the message is starred (the `\Flagged` flag).
    Starred,
}

impl SearchField {
    /// Serialized snake_case form (matches serde and the `field` JSON key).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
            Self::Cc => "cc",
            Self::Subject => "subject",
            Self::Body => "body",
            Self::Date => "date",
            Self::Tag => "tag",
            Self::HasAttachment => "has_attachment",
            Self::Folder => "folder",
            Self::Account => "account",
            Self::Read => "read",
            Self::Starred => "starred",
        }
    }
}

/// How the matched results are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    /// Newest first (the default).
    #[default]
    DateDesc,
    /// Oldest first.
    DateAsc,
    /// Subject A→Z (case-insensitive).
    SubjectAsc,
    /// Subject Z→A (case-insensitive).
    SubjectDesc,
}

impl SearchSort {
    /// Serialized form stored in the `sort` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DateDesc => "date_desc",
            Self::DateAsc => "date_asc",
            Self::SubjectAsc => "subject_asc",
            Self::SubjectDesc => "subject_desc",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        Ok(match value {
            "date_desc" => Self::DateDesc,
            "date_asc" => Self::DateAsc,
            "subject_asc" => Self::SubjectAsc,
            "subject_desc" => Self::SubjectDesc,
            _ => {
                return Err(MailError::Database {
                    message: "read_saved_search: unknown sort".to_owned(),
                });
            }
        })
    }
}

/// The structural narrowing applied before conditions are evaluated: which
/// folders are candidates at all. Serialized as JSON into the `scope` column.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchScope {
    /// Every folder of every account.
    #[default]
    AllFolders,
    /// A single folder.
    Folder {
        /// The folder to search.
        #[serde(rename = "mailboxId")]
        mailbox_id: MailboxId,
    },
    /// A folder and all of its descendants.
    FolderRecursive {
        /// The root folder to search under.
        #[serde(rename = "mailboxId")]
        mailbox_id: MailboxId,
    },
}

/// One condition of a search (an ordered value; no standalone id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCondition {
    /// The message part to test.
    pub field: SearchField,
    /// The comparison operator (reused from the filter model).
    pub op: ConditionOp,
    /// The operand text (interpreted per field/op).
    pub value: String,
}

/// A complete advanced-search query: the condition set, the scope, and the sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// How the conditions combine (all = AND, any = OR).
    pub match_type: MatchType,
    /// The conditions, in the order the user built them.
    pub conditions: Vec<SearchCondition>,
    /// The structural scope.
    pub scope: SearchScope,
    /// The result ordering.
    pub sort: SearchSort,
}

/// The serialized shape of the `query_json` column: the condition set only
/// (scope and sort are their own columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryJson {
    pub match_type: MatchType,
    pub conditions: Vec<SearchCondition>,
}

impl SearchQuery {
    /// Splits the query into the three persisted columns: the condition-set
    /// JSON, the scope JSON, and the sort string.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if serialization fails (it should not for
    /// these plain data types).
    pub(super) fn to_columns(&self) -> Result<(String, String, &'static str), MailError> {
        let query_json = serde_json::to_string(&QueryJson {
            match_type: self.match_type,
            conditions: self.conditions.clone(),
        })
        .map_err(|error| MailError::Database {
            message: format!("saved_search: encode query: {error}"),
        })?;
        let scope = serde_json::to_string(&self.scope).map_err(|error| MailError::Database {
            message: format!("saved_search: encode scope: {error}"),
        })?;
        Ok((query_json, scope, self.sort.as_str()))
    }

    /// Reassembles a query from its persisted columns.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if any column cannot be parsed.
    pub(super) fn from_columns(
        query_json: &str,
        scope: &str,
        sort: &str,
    ) -> Result<Self, MailError> {
        let query: QueryJson =
            serde_json::from_str(query_json).map_err(|error| MailError::Database {
                message: format!("saved_search: decode query: {error}"),
            })?;
        let scope: SearchScope =
            serde_json::from_str(scope).map_err(|error| MailError::Database {
                message: format!("saved_search: decode scope: {error}"),
            })?;
        Ok(Self {
            match_type: query.match_type,
            conditions: query.conditions,
            scope,
            sort: SearchSort::parse(sort)?,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn sort_forms_round_trip() {
        for value in ["date_desc", "date_asc", "subject_asc", "subject_desc"] {
            assert_eq!(SearchSort::parse(value).unwrap().as_str(), value);
        }
        assert!(SearchSort::parse("relevance").is_err());
    }

    #[test]
    fn query_columns_round_trip() {
        let query = SearchQuery {
            match_type: MatchType::Any,
            conditions: vec![
                SearchCondition {
                    field: SearchField::Subject,
                    op: ConditionOp::Contains,
                    value: "invoice".to_owned(),
                },
                SearchCondition {
                    field: SearchField::Starred,
                    op: ConditionOp::Is,
                    value: "true".to_owned(),
                },
            ],
            scope: SearchScope::FolderRecursive {
                mailbox_id: MailboxId::new(7),
            },
            sort: SearchSort::SubjectAsc,
        };
        let (query_json, scope, sort) = query.to_columns().unwrap();
        // The scope JSON uses the camelCase `mailboxId` key the DTO layer mirrors.
        assert!(scope.contains("\"mailboxId\":7"));
        let restored = SearchQuery::from_columns(&query_json, &scope, sort).unwrap();
        assert_eq!(restored, query);
    }

    #[test]
    fn scope_defaults_to_all_folders() {
        assert_eq!(SearchScope::default(), SearchScope::AllFolders);
        let json = serde_json::to_string(&SearchScope::AllFolders).unwrap();
        assert_eq!(json, "{\"kind\":\"all_folders\"}");
    }
}
