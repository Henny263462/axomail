//! The pure filter evaluator.
//!
//! [`FilterProgram::compile`] turns a list of [`Filter`]s into a matcher (any
//! `matches_regex` pattern is compiled once here, not per message), and
//! [`FilterProgram::evaluate`] runs it against one message's [`FilterMessageFacts`]
//! to produce the ordered actions to apply. Nothing here touches the database
//! or the network — it is a deterministic function of `(filters, facts)`, so the
//! whole engine is unit-testable in isolation.
//!
//! Matching semantics:
//! - Address fields (`from`/`to`/`cc`) and `tag` are multi-valued: a *positive*
//!   op (`contains`, `is`, `starts_with`, `ends_with`, `matches_regex`) matches
//!   when **any** value satisfies it; a *negative* op (`not_contains`, `is_not`)
//!   matches when **no** value satisfies its positive form.
//! - `subject` and `body` are single strings.
//! - `size` and `date` are numeric (`greater_than`/`less_than`/`is`/`is_not`);
//!   `date` values may be a Unix-seconds integer or a `YYYY-MM-DD` date.
//! - `has_attachment` is a boolean tested with `is`/`is_not`.
//! - Regex matching uses the `regex` crate, whose linear-time engine makes
//!   user-supplied patterns safe from catastrophic backtracking (ReDoS).

use regex::{Regex, RegexBuilder};

use super::types::{ConditionField, ConditionOp, Filter, FilterAction, MatchType};

/// The facts about one message the evaluator reads. Built by the facts loader
/// (`super::facts`) from storage, or by hand in tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterMessageFacts {
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
    /// Tag identifiers: each tag's keyword atom and display name (lowercased).
    pub tags: Vec<String>,
    /// Whether the message has at least one stored attachment.
    pub has_attachment: bool,
    /// Message size in bytes, if known.
    pub size: Option<u64>,
    /// Message date as Unix seconds, if known.
    pub date: Option<i64>,
}

/// A compiled, ready-to-run set of filters.
#[derive(Debug)]
pub struct FilterProgram {
    filters: Vec<CompiledFilter>,
}

/// One compiled filter.
#[derive(Debug)]
struct CompiledFilter {
    match_type: MatchType,
    stop_on_match: bool,
    conditions: Vec<CompiledCondition>,
    actions: Vec<FilterAction>,
}

/// One compiled condition: its field, op, operand, and (for `matches_regex`)
/// the compiled pattern — `None` when the pattern failed to compile, which
/// makes the condition simply never match.
#[derive(Debug)]
struct CompiledCondition {
    field: ConditionField,
    op: ConditionOp,
    value: String,
    regex: Option<Regex>,
}

impl FilterProgram {
    /// Compiles a rule list (already in evaluation order) into a program.
    #[must_use]
    pub fn compile(filters: &[Filter]) -> Self {
        let filters = filters
            .iter()
            .map(|filter| CompiledFilter {
                match_type: filter.match_type,
                stop_on_match: filter.stop_on_match,
                conditions: filter
                    .conditions
                    .iter()
                    .map(|condition| CompiledCondition {
                        field: condition.field,
                        op: condition.op,
                        value: condition.value.to_lowercase(),
                        regex: match condition.op {
                            // Compile the pattern verbatim with the
                            // case-insensitive flag — never lowercase the
                            // pattern text, which would corrupt uppercase escape
                            // classes (`\D`→`\d`, `\S`→`\s`, `\W`→`\w`, `\B`→`\b`)
                            // and silently invert a possibly-destructive rule.
                            // The haystack is already lowercased, so folding case
                            // only affects the pattern's literal characters.
                            ConditionOp::MatchesRegex => RegexBuilder::new(&condition.value)
                                .case_insensitive(true)
                                .build()
                                .ok(),
                            _ => None,
                        },
                    })
                    .collect(),
                actions: filter.actions.clone(),
            })
            .collect();
        Self { filters }
    }

    /// Evaluates every filter against `facts` and returns the actions to apply,
    /// in order. Evaluation halts after the first matching filter that has
    /// `stop_on_match` set or emits a [`ActionKind::Stop`](super::types::ActionKind::Stop).
    #[must_use]
    pub fn evaluate(&self, facts: &FilterMessageFacts) -> Vec<FilterAction> {
        let mut actions = Vec::new();
        for filter in &self.filters {
            if !filter.matches(facts) {
                continue;
            }
            let stops = filter.stop_on_match
                || filter
                    .actions
                    .iter()
                    .any(|action| action.kind == super::types::ActionKind::Stop);
            actions.extend(filter.actions.iter().cloned());
            if stops {
                break;
            }
        }
        actions
    }

    /// The number of compiled filters (for reporting/tests).
    #[must_use]
    pub fn len(&self) -> usize {
        self.filters.len()
    }

    /// Whether the program has no filters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

impl CompiledFilter {
    /// Whether this filter matches the facts, honoring its `match_type`. A
    /// filter with no conditions never matches (a guard against a rule that
    /// would fire on every message).
    fn matches(&self, facts: &FilterMessageFacts) -> bool {
        if self.conditions.is_empty() {
            return false;
        }
        match self.match_type {
            MatchType::All => self.conditions.iter().all(|c| c.matches(facts)),
            MatchType::Any => self.conditions.iter().any(|c| c.matches(facts)),
        }
    }
}

impl CompiledCondition {
    /// Whether this condition holds for the facts.
    fn matches(&self, facts: &FilterMessageFacts) -> bool {
        match self.field {
            ConditionField::From => self.match_strings(&facts.from),
            ConditionField::To => self.match_strings(&facts.to),
            ConditionField::Cc => self.match_strings(&facts.cc),
            ConditionField::Tag => self.match_strings(&facts.tags),
            ConditionField::Subject => self.match_strings(std::slice::from_ref(&facts.subject)),
            ConditionField::Body => self.match_strings(std::slice::from_ref(&facts.body)),
            ConditionField::HasAttachment => self.match_bool(facts.has_attachment),
            ConditionField::Size => self.match_number(facts.size.map(|s| s as i64)),
            ConditionField::Date => self.match_number(facts.date),
        }
    }

    /// Multi-valued string match with positive/negative aggregation.
    fn match_strings(&self, values: &[String]) -> bool {
        match self.op {
            ConditionOp::NotContains => !values.iter().any(|v| self.contains(v)),
            ConditionOp::IsNot => !values.iter().any(|v| self.equals(v)),
            ConditionOp::Contains => values.iter().any(|v| self.contains(v)),
            ConditionOp::Is => values.iter().any(|v| self.equals(v)),
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

    /// Numeric field match. `date` operands accept `YYYY-MM-DD`; both accept a
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
        if self.field == ConditionField::Date {
            return parse_iso_date(self.value.trim());
        }
        None
    }

    /// Case-insensitive `contains` (both sides already lowercased).
    fn contains(&self, haystack: &str) -> bool {
        haystack.contains(&self.value)
    }

    /// Case-insensitive equality (both sides already lowercased).
    fn equals(&self, haystack: &str) -> bool {
        haystack == self.value
    }
}

/// Parses a truthy/falsy operand for the `has_attachment` field.
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
    // The evaluator's matching behaviour is covered end-to-end in
    // `tests/filters.rs`; these inline tests cover the private date helpers the
    // integration test cannot reach.
    use super::*;

    #[test]
    fn iso_date_parses_to_utc_midnight() {
        assert_eq!(parse_iso_date("1970-01-01"), Some(0));
        assert_eq!(parse_iso_date("1970-01-02"), Some(86_400));
        assert_eq!(parse_iso_date("2024-01-01"), Some(1_704_067_200));
    }

    #[test]
    fn iso_date_rejects_malformed() {
        assert_eq!(parse_iso_date("2024-13-01"), None);
        assert_eq!(parse_iso_date("2024-01-40"), None);
        assert_eq!(parse_iso_date("2024-01"), None);
        assert_eq!(parse_iso_date("not-a-date"), None);
    }
}
