//! IMAP keywords and the additive flag-delta write contract.
//!
//! A [`Keyword`] is a validated custom (non-system) IMAP flag: an RFC 3501
//! atom, stored case-preserving but compared case-insensitively so that
//! `$Junk` and `$junk` are one keyword. [`FlagDelta`] replaces the old
//! REPLACE-form `set_flags`: a write names only the flags it adds and the
//! flags it removes (`STORE +FLAGS` / `-FLAGS`), so it can never clobber a
//! flag or keyword the client did not mention. That is the invariant Wave 2
//! track 2.1 (write-back) relies on to avoid dropping server-side keywords.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// System-flag base names a keyword must never shadow (compared ASCII-caseless).
///
/// The `\`-prefixed system flags cannot be atoms anyway (`\` is excluded from
/// the atom charset below), but rejecting the bare names too keeps a keyword
/// from masquerading as a system flag in storage or the UI.
const RESERVED_SYSTEM_NAMES: [&str; 6] =
    ["seen", "answered", "flagged", "draft", "deleted", "recent"];

/// A validated IMAP keyword (RFC 3501 `atom`).
///
/// Case is preserved on the wire (`$Forwarded` stays `$Forwarded`), but
/// equality, ordering, and hashing are ASCII-case-insensitive, so a keyword
/// set never holds two spellings of the same keyword.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Keyword(String);

impl Keyword {
    /// Parses and validates a keyword against the RFC 3501 atom charset.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when the input is empty, is not
    /// pure ASCII, contains a control or atom-special character
    /// (`( ) { SP % * " \ ]`), or matches a reserved system-flag name.
    pub fn parse(input: &str) -> Result<Self, MailError> {
        let invalid = |message: &str| MailError::Configuration {
            message: format!("invalid keyword: {message}"),
        };

        if input.is_empty() {
            return Err(invalid("empty input"));
        }
        for ch in input.chars() {
            if !ch.is_ascii() {
                return Err(invalid("must be ASCII (RFC 3501 atom)"));
            }
            if ch.is_ascii_control() {
                return Err(invalid("contains a control character"));
            }
            if matches!(ch, '(' | ')' | '{' | ' ' | '%' | '*' | '"' | '\\' | ']') {
                return Err(invalid("contains an atom-special character"));
            }
        }
        if RESERVED_SYSTEM_NAMES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(input))
        {
            return Err(invalid("shadows a reserved system-flag name"));
        }
        Ok(Self(input.to_owned()))
    }

    /// Returns the keyword's original-case text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for Keyword {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for Keyword {}

impl std::hash::Hash for Keyword {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the ASCII-lowercased bytes so equal keywords hash equally.
        for byte in self.0.bytes() {
            state.write_u8(byte.to_ascii_lowercase());
        }
    }
}

impl PartialOrd for Keyword {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Keyword {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(other.0.bytes().map(|byte| byte.to_ascii_lowercase()))
    }
}

impl TryFrom<String> for Keyword {
    type Error = MailError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Keyword> for String {
    fn from(value: Keyword) -> Self {
        value.0
    }
}

impl std::fmt::Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The set of IMAP system flags a value covers (`true` = flag present).
///
/// Used both as the system-flag component of a [`FlagSet`] and, via
/// [`MessageFlags::system`](crate::MessageFlags::system), as a mask of a
/// message's current system flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemFlags {
    /// `\Seen`.
    pub seen: bool,
    /// `\Answered`.
    pub answered: bool,
    /// `\Flagged`.
    pub flagged: bool,
    /// `\Draft`.
    pub draft: bool,
    /// `\Deleted`.
    pub deleted: bool,
}

impl SystemFlags {
    /// Returns `true` when no system flag is set.
    #[must_use]
    pub fn is_empty(self) -> bool {
        !(self.seen || self.answered || self.flagged || self.draft || self.deleted)
    }
}

/// A set of flags — system flags plus IMAP keywords — as one value.
///
/// This is the unit of a [`FlagDelta`]'s `add`/`remove` sides. It carries no
/// notion of "replace": it is only ever interpreted additively.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagSet {
    /// System flags in this set.
    pub system: SystemFlags,
    /// Keywords in this set (case-insensitive, deduplicated, ordered).
    pub keywords: BTreeSet<Keyword>,
}

impl FlagSet {
    /// Returns `true` when the set names no flags at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.system.is_empty() && self.keywords.is_empty()
    }

    /// A set of only the given system flags.
    #[must_use]
    pub fn from_system(system: SystemFlags) -> Self {
        Self {
            system,
            keywords: BTreeSet::new(),
        }
    }

    /// A set of a single keyword.
    #[must_use]
    pub fn from_keyword(keyword: Keyword) -> Self {
        Self {
            system: SystemFlags::default(),
            keywords: BTreeSet::from([keyword]),
        }
    }
}

/// An additive flag change: flags to add and flags to remove.
///
/// Replaces the old REPLACE-form `set_flags`. An adapter applies `add` with
/// `STORE +FLAGS` and `remove` with `STORE -FLAGS` (empty sides skipped), so a
/// write never disturbs flags or keywords outside the delta. Track 2.1 builds
/// these from the pending-op queue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagDelta {
    /// Flags and keywords to add.
    pub add: FlagSet,
    /// Flags and keywords to remove.
    pub remove: FlagSet,
}

impl FlagDelta {
    /// Returns `true` when the delta changes nothing (both sides empty).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn accepts_common_keywords() {
        for input in [
            "$Junk",
            "$NotJunk",
            "$Forwarded",
            "NonJunk",
            "Label1",
            "$MDNSent",
        ] {
            let keyword = Keyword::parse(input).unwrap();
            assert_eq!(keyword.as_str(), input, "case is preserved for {input}");
        }
    }

    #[test]
    fn rejects_atom_specials_controls_and_non_ascii() {
        for input in [
            "",
            "has space",
            "paren(",
            "brace{",
            "star*",
            "percent%",
            "quote\"",
            "back\\slash",
            "bracket]",
            "tab\ttab",
            "über",
        ] {
            assert!(Keyword::parse(input).is_err(), "should reject {input:?}");
        }
    }

    #[test]
    fn rejects_system_flag_names_case_insensitively() {
        for input in [
            "Seen", "seen", "ANSWERED", "Flagged", "draft", "Deleted", "recent",
        ] {
            assert!(Keyword::parse(input).is_err(), "should reject {input:?}");
        }
    }

    #[test]
    fn equality_ordering_and_dedup_are_case_insensitive() {
        assert_eq!(
            Keyword::parse("$Junk").unwrap(),
            Keyword::parse("$junk").unwrap()
        );
        let mut set = BTreeSet::new();
        set.insert(Keyword::parse("$Junk").unwrap());
        set.insert(Keyword::parse("$JUNK").unwrap());
        set.insert(Keyword::parse("$Forwarded").unwrap());
        assert_eq!(set.len(), 2, "case variants collapse to one keyword");
    }

    #[test]
    fn serde_validates_on_deserialize() {
        let ok: Result<Keyword, _> = serde_json::from_str("\"$Junk\"");
        assert!(ok.is_ok());
        let bad: Result<Keyword, _> = serde_json::from_str("\"bad flag\"");
        assert!(bad.is_err());
    }

    #[test]
    fn flag_delta_and_sets_report_emptiness() {
        assert!(FlagDelta::default().is_empty());
        let delta = FlagDelta {
            add: FlagSet::from_system(SystemFlags {
                seen: true,
                ..SystemFlags::default()
            }),
            remove: FlagSet::from_keyword(Keyword::parse("$Junk").unwrap()),
        };
        assert!(!delta.is_empty());
        assert!(!delta.add.is_empty());
        assert!(!delta.remove.is_empty());
        assert!(FlagSet::default().is_empty());
    }
}
