//! Value types for the filter repository: the filter identifier, the
//! CHECK-mirrored enums (match type, condition field/op, action kind), and the
//! new/persisted filter structs.
//!
//! Conditions and actions are modeled without their own ids: they are always
//! loaded as part of a [`Filter`] and replaced as a whole set on update, so the
//! evaluator and UI treat them as ordered value lists.

use axomail_core::{AccountId, MailError};
use serde::{Deserialize, Serialize};

/// Identifier of a persisted [`Filter`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FilterId(i64);

impl FilterId {
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

impl std::fmt::Display for FilterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How a filter's conditions combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    /// Every condition must match (logical AND).
    All,
    /// At least one condition must match (logical OR).
    Any,
}

impl MatchType {
    /// Serialized form stored in the `match_type` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Any => "any",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value {
            "all" => Ok(Self::All),
            "any" => Ok(Self::Any),
            _ => Err(unknown("match type")),
        }
    }
}

/// Which part of a message a condition tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionField {
    /// The `From` address(es).
    From,
    /// The `To` address(es).
    To,
    /// The `Cc` address(es).
    Cc,
    /// The subject line.
    Subject,
    /// The plain-text body (locally indexed; preview until fully downloaded).
    Body,
    /// Whether the message carries any attachment.
    HasAttachment,
    /// The message size in bytes.
    Size,
    /// The message date (Unix seconds).
    Date,
    /// A tag / IMAP keyword on the message.
    Tag,
}

impl ConditionField {
    /// Serialized form stored in the `field` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
            Self::Cc => "cc",
            Self::Subject => "subject",
            Self::Body => "body",
            Self::HasAttachment => "has_attachment",
            Self::Size => "size",
            Self::Date => "date",
            Self::Tag => "tag",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        Ok(match value {
            "from" => Self::From,
            "to" => Self::To,
            "cc" => Self::Cc,
            "subject" => Self::Subject,
            "body" => Self::Body,
            "has_attachment" => Self::HasAttachment,
            "size" => Self::Size,
            "date" => Self::Date,
            "tag" => Self::Tag,
            _ => return Err(unknown("condition field")),
        })
    }
}

/// How a condition compares its field to its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    /// Field contains the value (case-insensitive substring).
    Contains,
    /// Field does not contain the value.
    NotContains,
    /// Field equals the value (case-insensitive).
    Is,
    /// Field does not equal the value.
    IsNot,
    /// Field starts with the value.
    StartsWith,
    /// Field ends with the value.
    EndsWith,
    /// Field matches the value as a regular expression.
    MatchesRegex,
    /// Numeric field is greater than the value.
    GreaterThan,
    /// Numeric field is less than the value.
    LessThan,
}

impl ConditionOp {
    /// Serialized form stored in the `op` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::NotContains => "not_contains",
            Self::Is => "is",
            Self::IsNot => "is_not",
            Self::StartsWith => "starts_with",
            Self::EndsWith => "ends_with",
            Self::MatchesRegex => "matches_regex",
            Self::GreaterThan => "greater_than",
            Self::LessThan => "less_than",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        Ok(match value {
            "contains" => Self::Contains,
            "not_contains" => Self::NotContains,
            "is" => Self::Is,
            "is_not" => Self::IsNot,
            "starts_with" => Self::StartsWith,
            "ends_with" => Self::EndsWith,
            "matches_regex" => Self::MatchesRegex,
            "greater_than" => Self::GreaterThan,
            "less_than" => Self::LessThan,
            _ => return Err(unknown("condition op")),
        })
    }
}

/// What a filter action does when its filter matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Move the message to the mailbox named by `param` (a mailbox id).
    Move,
    /// Copy the message to the mailbox named by `param` (a mailbox id).
    Copy,
    /// Apply the tag named by `param` (a tag name).
    Tag,
    /// Star (flag) the message.
    Star,
    /// Mark the message read.
    MarkRead,
    /// Mark the message unread.
    MarkUnread,
    /// Mark the message as junk (adds the `$Junk` keyword).
    MarkJunk,
    /// Move the message to the account's Trash.
    DeleteToTrash,
    /// Forward the message to the address in `param` (via the outbox).
    Forward,
    /// Set the message priority (`param` = level). No local priority store yet.
    SetPriority,
    /// Stop evaluating further filters for this message.
    Stop,
}

impl ActionKind {
    /// Serialized form stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Copy => "copy",
            Self::Tag => "tag",
            Self::Star => "star",
            Self::MarkRead => "mark_read",
            Self::MarkUnread => "mark_unread",
            Self::MarkJunk => "mark_junk",
            Self::DeleteToTrash => "delete_to_trash",
            Self::Forward => "forward",
            Self::SetPriority => "set_priority",
            Self::Stop => "stop",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        Ok(match value {
            "move" => Self::Move,
            "copy" => Self::Copy,
            "tag" => Self::Tag,
            "star" => Self::Star,
            "mark_read" => Self::MarkRead,
            "mark_unread" => Self::MarkUnread,
            "mark_junk" => Self::MarkJunk,
            "delete_to_trash" => Self::DeleteToTrash,
            "forward" => Self::Forward,
            "set_priority" => Self::SetPriority,
            "stop" => Self::Stop,
            _ => return Err(unknown("action kind")),
        })
    }
}

/// One condition of a filter (an ordered value; no standalone id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterCondition {
    /// The message part to test.
    pub field: ConditionField,
    /// The comparison operator.
    pub op: ConditionOp,
    /// The operand text (interpreted per field/op).
    pub value: String,
}

/// One action of a filter (an ordered value; no standalone id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterAction {
    /// What to do.
    pub kind: ActionKind,
    /// The action operand (may be empty).
    pub param: String,
}

/// A persisted filter with its conditions and actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// Row identifier.
    pub id: FilterId,
    /// Owning account, or `None` for a global (all-accounts) rule.
    pub account: Option<AccountId>,
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
    /// Conditions in position order.
    pub conditions: Vec<FilterCondition>,
    /// Actions in position order.
    pub actions: Vec<FilterAction>,
}

/// The editable content of a filter (used for both create and update).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterInput {
    /// Owning account, or `None` for a global rule.
    pub account: Option<AccountId>,
    /// User-visible name.
    pub name: String,
    /// Whether the rule is active.
    pub enabled: bool,
    /// How the conditions combine.
    pub match_type: MatchType,
    /// Whether a match halts evaluation of later filters.
    pub stop_on_match: bool,
    /// Conditions in position order.
    pub conditions: Vec<FilterCondition>,
    /// Actions in position order.
    pub actions: Vec<FilterAction>,
}

/// Maps an unrecognized enum value read from the database to a value-free error.
fn unknown(what: &str) -> MailError {
    MailError::Database {
        message: format!("read_filter: unknown {what}"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn enum_forms_round_trip() {
        for value in ["all", "any"] {
            assert_eq!(MatchType::parse(value).unwrap().as_str(), value);
        }
        for value in [
            "from",
            "to",
            "cc",
            "subject",
            "body",
            "has_attachment",
            "size",
            "date",
            "tag",
        ] {
            assert_eq!(ConditionField::parse(value).unwrap().as_str(), value);
        }
        for value in [
            "contains",
            "not_contains",
            "is",
            "is_not",
            "starts_with",
            "ends_with",
            "matches_regex",
            "greater_than",
            "less_than",
        ] {
            assert_eq!(ConditionOp::parse(value).unwrap().as_str(), value);
        }
        for value in [
            "move",
            "copy",
            "tag",
            "star",
            "mark_read",
            "mark_unread",
            "mark_junk",
            "delete_to_trash",
            "forward",
            "set_priority",
            "stop",
        ] {
            assert_eq!(ActionKind::parse(value).unwrap().as_str(), value);
        }
    }

    #[test]
    fn unknown_enum_values_error() {
        assert!(MatchType::parse("some").is_err());
        assert!(ConditionField::parse("header").is_err());
        assert!(ConditionOp::parse("regex").is_err());
        assert!(ActionKind::parse("bounce").is_err());
    }
}
