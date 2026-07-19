//! Subject-line normalization for replies and forwards.
//!
//! The prefix check is locale-agnostic and case-insensitive: an existing
//! `Re:`/`Fwd:` (in any casing, plus the `Fw:` variant for forwards) is never
//! doubled. Matching is done on ASCII bytes so it never depends on the host
//! locale and never panics on a multi-byte subject.

/// Which reply-style prefix a subject should carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prefix {
    /// `Re:` — a reply or reply-all.
    Reply,
    /// `Fwd:` — a forward.
    Forward,
}

impl Prefix {
    /// The canonical prefix this variant applies (with a trailing space).
    fn canonical(self) -> &'static str {
        match self {
            Self::Reply => "Re: ",
            Self::Forward => "Fwd: ",
        }
    }

    /// The ASCII prefixes (case-insensitive) that count as "already prefixed",
    /// so re-prefixing is skipped.
    fn existing(self) -> &'static [&'static str] {
        match self {
            Self::Reply => &["re:"],
            // Both the modern `Fwd:` and the older `Fw:` are recognized so a
            // forward of a forward is not doubly prefixed.
            Self::Forward => &["fwd:", "fw:"],
        }
    }
}

/// Normalizes a subject for a reply or forward, adding the canonical prefix
/// unless one is already present (case-insensitively).
///
/// A `None` or blank source subject yields the bare prefix (e.g. `Re:`), which
/// mirrors how mail clients present a reply to a subject-less message.
pub(crate) fn normalize_subject(subject: Option<&str>, prefix: Prefix) -> String {
    let trimmed = subject.unwrap_or_default().trim();
    if prefix.existing().iter().any(|p| starts_with_ci(trimmed, p)) {
        return trimmed.to_owned();
    }
    if trimmed.is_empty() {
        return prefix.canonical().trim_end().to_owned();
    }
    format!("{}{trimmed}", prefix.canonical())
}

/// ASCII case-insensitive prefix test that never panics on a char boundary.
fn starts_with_ci(haystack: &str, prefix: &str) -> bool {
    let (h, p) = (haystack.as_bytes(), prefix.as_bytes());
    h.len() >= p.len() && h[..p.len()].eq_ignore_ascii_case(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_adds_re_prefix() {
        assert_eq!(
            normalize_subject(Some("Lunch?"), Prefix::Reply),
            "Re: Lunch?"
        );
    }

    #[test]
    fn reply_does_not_double_prefix_any_casing() {
        assert_eq!(
            normalize_subject(Some("Re: Lunch?"), Prefix::Reply),
            "Re: Lunch?"
        );
        assert_eq!(
            normalize_subject(Some("RE: Lunch?"), Prefix::Reply),
            "RE: Lunch?"
        );
        assert_eq!(
            normalize_subject(Some("re: Lunch?"), Prefix::Reply),
            "re: Lunch?"
        );
    }

    #[test]
    fn forward_adds_fwd_prefix() {
        assert_eq!(
            normalize_subject(Some("Report"), Prefix::Forward),
            "Fwd: Report"
        );
    }

    #[test]
    fn forward_recognizes_existing_fwd_and_fw() {
        assert_eq!(
            normalize_subject(Some("Fwd: Report"), Prefix::Forward),
            "Fwd: Report"
        );
        assert_eq!(
            normalize_subject(Some("FW: Report"), Prefix::Forward),
            "FW: Report"
        );
    }

    #[test]
    fn a_reply_prefix_does_not_satisfy_a_forward() {
        // "Re:" is not a forward prefix, so forwarding a reply prefixes Fwd:.
        assert_eq!(
            normalize_subject(Some("Re: Report"), Prefix::Forward),
            "Fwd: Re: Report"
        );
    }

    #[test]
    fn blank_or_missing_subject_yields_the_bare_prefix() {
        assert_eq!(normalize_subject(None, Prefix::Reply), "Re:");
        assert_eq!(normalize_subject(Some("   "), Prefix::Forward), "Fwd:");
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_before_prefixing() {
        assert_eq!(
            normalize_subject(Some("  Lunch?  "), Prefix::Reply),
            "Re: Lunch?"
        );
    }
}
