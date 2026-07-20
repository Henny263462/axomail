//! The pure advanced-search evaluator.
//!
//! [`SearchProgram::compile`] turns a [`SearchQuery`] into a matcher (compiling
//! any `matches_regex` pattern once), and [`SearchProgram::matches`] runs it
//! against one message's [`SearchMessageFacts`] to a boolean. Nothing here
//! touches the database: it is a deterministic function of `(query, facts)`, so
//! it is unit-testable in isolation.
//!
//! The op-level matching semantics are the same the filter evaluator uses
//! (case-insensitive substring / equality / prefix / suffix / linear-time regex
//! for text; `greater_than`/`less_than`/`is`/`is_not` for numbers; `is`/`is_not`
//! for booleans), applied here to the search-only facets as well
//! (folder/account are strings, read/starred are booleans).

use regex::{Regex, RegexBuilder};

use super::query::{ConditionOp, MatchType, SearchCondition, SearchField, SearchQuery};

/// The facts about one message the evaluator reads. Built by the runner
/// ([`super::run`]) from storage, or by hand in tests. Every string field is
/// lowercased so matching is case-insensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchMessageFacts {
    /// Decoded subject.
    pub subject: String,
    /// `From` addresses, each rendered as `name email` (lowercased).
    pub from: Vec<String>,
    /// `To` addresses.
    pub to: Vec<String>,
    /// `Cc` addresses.
    pub cc: Vec<String>,
    /// Locally indexed plain-text body (preview until fully downloaded).
    pub body: String,
    /// Each tag's keyword atom and display name (lowercased).
    pub tags: Vec<String>,
    /// The folder's display name (lowercased).
    pub folder: String,
    /// The owning account's display name and email (lowercased).
    pub account: String,
    /// Whether the message has at least one stored attachment.
    pub has_attachment: bool,
    /// Whether the message is read (`\Seen`).
    pub read: bool,
    /// Whether the message is starred (`\Flagged`).
    pub starred: bool,
    /// Message date as Unix seconds, if known.
    pub date: Option<i64>,
}

/// A compiled, ready-to-run search query.
#[derive(Debug)]
pub struct SearchProgram {
    match_type: MatchType,
    conditions: Vec<CompiledCondition>,
}

/// One compiled condition: its field, op, lowercased operand, and (for
/// `matches_regex`) the compiled pattern — `None` when it failed to compile,
/// which makes the condition simply never match.
#[derive(Debug)]
struct CompiledCondition {
    field: SearchField,
    op: ConditionOp,
    value: String,
    regex: Option<Regex>,
}

impl SearchProgram {
    /// Compiles a query into a matcher.
    #[must_use]
    pub fn compile(query: &SearchQuery) -> Self {
        let conditions = query
            .conditions
            .iter()
            .map(CompiledCondition::compile)
            .collect();
        Self {
            match_type: query.match_type,
            conditions,
        }
    }

    /// Whether the query matches the facts, honoring its `match_type`. A query
    /// with no conditions matches every message (an advanced search that only
    /// narrows by scope is valid — it returns everything in scope).
    #[must_use]
    pub fn matches(&self, facts: &SearchMessageFacts) -> bool {
        if self.conditions.is_empty() {
            return true;
        }
        match self.match_type {
            MatchType::All => self.conditions.iter().all(|c| c.matches(facts)),
            MatchType::Any => self.conditions.iter().any(|c| c.matches(facts)),
        }
    }
}

impl CompiledCondition {
    fn compile(condition: &SearchCondition) -> Self {
        let regex = match condition.op {
            // Compile the pattern verbatim with the case-insensitive flag — never
            // lowercase the pattern text, which would corrupt uppercase escape
            // classes (`\D`→`\d`, `\S`→`\s`, `\W`→`\w`, `\B`→`\b`). The haystack
            // is already lowercased, so folding case only affects literals.
            ConditionOp::MatchesRegex => RegexBuilder::new(&condition.value)
                .case_insensitive(true)
                .build()
                .ok(),
            _ => None,
        };
        Self {
            field: condition.field,
            op: condition.op,
            value: condition.value.to_lowercase(),
            regex,
        }
    }

    /// Whether this condition holds for the facts.
    fn matches(&self, facts: &SearchMessageFacts) -> bool {
        match self.field {
            SearchField::From => self.match_strings(&facts.from),
            SearchField::To => self.match_strings(&facts.to),
            SearchField::Cc => self.match_strings(&facts.cc),
            SearchField::Tag => self.match_strings(&facts.tags),
            SearchField::Subject => self.match_strings(std::slice::from_ref(&facts.subject)),
            SearchField::Body => self.match_strings(std::slice::from_ref(&facts.body)),
            SearchField::Folder => self.match_strings(std::slice::from_ref(&facts.folder)),
            SearchField::Account => self.match_strings(std::slice::from_ref(&facts.account)),
            SearchField::HasAttachment => self.match_bool(facts.has_attachment),
            SearchField::Read => self.match_bool(facts.read),
            SearchField::Starred => self.match_bool(facts.starred),
            SearchField::Date => self.match_number(facts.date),
        }
    }

    /// Multi-valued string match with positive/negative aggregation.
    fn match_strings(&self, values: &[String]) -> bool {
        match self.op {
            ConditionOp::NotContains => !values.iter().any(|v| v.contains(&self.value)),
            ConditionOp::IsNot => !values.contains(&self.value),
            ConditionOp::Contains => values.iter().any(|v| v.contains(&self.value)),
            ConditionOp::Is => values.contains(&self.value),
            ConditionOp::StartsWith => values.iter().any(|v| v.starts_with(&self.value)),
            ConditionOp::EndsWith => values.iter().any(|v| v.ends_with(&self.value)),
            ConditionOp::MatchesRegex => match &self.regex {
                Some(re) => values.iter().any(|v| re.is_match(v)),
                None => false,
            },
            // Numeric ops are not meaningful on text fields.
            ConditionOp::GreaterThan | ConditionOp::LessThan => false,
        }
    }

    /// Boolean field match (`is`/`is_not` against a parsed truthy value).
    fn match_bool(&self, actual: bool) -> bool {
        let Some(expected) = parse_bool(&self.value) else {
            return false;
        };
        match self.op {
            ConditionOp::Is => actual == expected,
            ConditionOp::IsNot => actual != expected,
            _ => false,
        }
    }

    /// Numeric field match. `date` operands accept `YYYY-MM-DD` as well as a
    /// plain integer. A field with no known value never matches.
    fn match_number(&self, actual: Option<i64>) -> bool {
        let (Some(actual), Some(expected)) = (actual, self.numeric_operand()) else {
            return false;
        };
        match self.op {
            ConditionOp::GreaterThan => actual > expected,
            ConditionOp::LessThan => actual < expected,
            ConditionOp::Is => actual == expected,
            ConditionOp::IsNot => actual != expected,
            _ => false,
        }
    }

    /// Parses the operand as a number, accepting `date`-style `YYYY-MM-DD`.
    fn numeric_operand(&self) -> Option<i64> {
        if let Ok(n) = self.value.trim().parse::<i64>() {
            return Some(n);
        }
        if self.field == SearchField::Date {
            return parse_iso_date(self.value.trim());
        }
        None
    }
}

/// Parses a truthy/falsy operand for a boolean field.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Parses a `YYYY-MM-DD` date to Unix seconds at UTC midnight, dependency-free.
fn parse_iso_date(value: &str) -> Option<i64> {
    let mut parts = value.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400)
}

/// Days since 1970-01-01 for a proleptic-Gregorian date (Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::super::query::{SearchScope, SearchSort};
    use super::*;

    fn query(match_type: MatchType, conditions: Vec<SearchCondition>) -> SearchQuery {
        SearchQuery {
            match_type,
            conditions,
            scope: SearchScope::AllFolders,
            sort: SearchSort::DateDesc,
        }
    }

    fn cond(field: SearchField, op: ConditionOp, value: &str) -> SearchCondition {
        SearchCondition {
            field,
            op,
            value: value.to_owned(),
        }
    }

    fn facts() -> SearchMessageFacts {
        SearchMessageFacts {
            subject: "quarterly invoice".to_owned(),
            from: vec!["ada lovelace ada@example.org".to_owned()],
            to: vec!["klaus@theferbers.eu".to_owned()],
            cc: Vec::new(),
            body: "please find the attached invoice".to_owned(),
            tags: vec!["$work".to_owned(), "work".to_owned()],
            folder: "inbox".to_owned(),
            account: "personal klaus@theferbers.eu".to_owned(),
            has_attachment: true,
            read: false,
            starred: true,
            date: Some(1_704_067_200), // 2024-01-01
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(SearchProgram::compile(&query(MatchType::All, vec![])).matches(&facts()));
    }

    #[test]
    fn all_requires_every_condition() {
        let program = SearchProgram::compile(&query(
            MatchType::All,
            vec![
                cond(SearchField::Subject, ConditionOp::Contains, "invoice"),
                cond(SearchField::Starred, ConditionOp::Is, "true"),
            ],
        ));
        assert!(program.matches(&facts()));

        let miss = SearchProgram::compile(&query(
            MatchType::All,
            vec![
                cond(SearchField::Subject, ConditionOp::Contains, "invoice"),
                cond(SearchField::Read, ConditionOp::Is, "true"),
            ],
        ));
        assert!(!miss.matches(&facts()));
    }

    #[test]
    fn any_requires_one_condition() {
        let program = SearchProgram::compile(&query(
            MatchType::Any,
            vec![
                cond(SearchField::Read, ConditionOp::Is, "true"),
                cond(SearchField::Folder, ConditionOp::Is, "inbox"),
            ],
        ));
        assert!(program.matches(&facts()));
    }

    #[test]
    fn account_and_date_facets() {
        let program = SearchProgram::compile(&query(
            MatchType::All,
            vec![
                cond(SearchField::Account, ConditionOp::Contains, "personal"),
                cond(SearchField::Date, ConditionOp::GreaterThan, "2023-12-31"),
            ],
        ));
        assert!(program.matches(&facts()));
    }

    #[test]
    fn regex_case_class_is_not_corrupted() {
        // `\D` (non-digit) must survive compilation; lowercasing it to `\d` would
        // invert the match. The subject has letters, so `\D` matches.
        let program = SearchProgram::compile(&query(
            MatchType::All,
            vec![cond(SearchField::Subject, ConditionOp::MatchesRegex, r"\D")],
        ));
        assert!(program.matches(&facts()));
    }
}
