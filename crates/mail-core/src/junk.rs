//! Dependency-free Bayesian junk (spam) classifier — a Graham-style token
//! filter.
//!
//! The classifier is intentionally pure: it owns tokenization and the scoring
//! math but holds no state and touches no storage. A caller (the storage layer)
//! supplies, for the distinct tokens of a message, their per-account corpus
//! counts ([`TokenStat`]) and the corpus message totals ([`Corpus`]); [`score`]
//! returns a spam probability in `0.0..=1.0`.
//!
//! Scoring follows Paul Graham's "A Plan for Spam" with the common refinements:
//! each token's spamminess is derived from how often it appears in spam versus
//! ham messages (ham counts doubled to bias against false positives), the most
//! decisive tokens dominate, and their probabilities are combined
//! multiplicatively. With no training data — or a message whose tokens were
//! never seen — the classifier has no opinion and returns [`NEUTRAL_SCORE`],
//! which keeps a fresh install from auto-filing mail before the user has taught
//! it anything.

use std::collections::BTreeSet;

use crate::address::Address;

/// The score returned when there is no evidence either way: an empty corpus, or
/// a message all of whose tokens are unknown. Exactly `0.5` — no lean.
pub const NEUTRAL_SCORE: f64 = 0.5;

/// Longest token kept; longer runs are almost always opaque ids or base64 and
/// carry no useful signal.
const MAX_TOKEN_LEN: usize = 40;
/// How many of the most decisive tokens contribute to the combined score.
const SIGNIFICANT_TOKENS: usize = 15;
/// Floor and ceiling clamped onto each token probability so a single rare token
/// can never force the combined score fully to 0 or 1.
const MIN_PROB: f64 = 0.01;
/// See [`MIN_PROB`].
const MAX_PROB: f64 = 0.99;
/// Ham counts are weighted this much more than spam counts when deriving a
/// token's spamminess, biasing the filter against false positives (Graham).
const HAM_BIAS: f64 = 2.0;

/// Per-token corpus counts: how many trained ham and spam messages a token
/// appeared in (each message counted at most once per token).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenStat {
    /// Ham (not-junk) messages the token appeared in.
    pub ham: u32,
    /// Spam (junk) messages the token appeared in.
    pub spam: u32,
}

/// Corpus-level totals: the number of ham and spam messages trained so far.
/// These are the denominators of the per-token spamminess estimate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Corpus {
    /// Total ham (not-junk) messages trained.
    pub ham_messages: u32,
    /// Total spam (junk) messages trained.
    pub spam_messages: u32,
}

impl Corpus {
    /// Whether no message has been trained in either class yet.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.ham_messages == 0 && self.spam_messages == 0
    }
}

/// Splits `text` into lowercased tokens.
///
/// A token is a maximal run of alphanumerics (Unicode-aware, so non-English
/// mail still tokenizes) plus the intra-word characters `$ ' - _`, which keep
/// spam-signal shapes like `$$$` and `low-cost` intact. Runs longer than
/// [`MAX_TOKEN_LEN`] or carrying no alphanumeric/`$` character are dropped.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '$' | '\'' | '-' | '_')))
        .filter(|run| !run.is_empty())
        .map(str::to_lowercase)
        .filter(|token| {
            token.chars().count() <= MAX_TOKEN_LEN
                && token.chars().any(|c| c.is_alphanumeric() || c == '$')
        })
        .collect()
}

/// Builds the distinct token set of a message from its subject, its addresses
/// (display names and email strings), and a body/preview text. The set is
/// deduplicated, so each token contributes at most once (Graham's rule) and the
/// same set feeds both training and scoring for a consistent view.
#[must_use]
pub fn message_tokens(
    subject: Option<&str>,
    addresses: &[Address],
    body: &str,
) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    if let Some(subject) = subject {
        set.extend(tokenize(subject));
    }
    for address in addresses {
        if let Some(name) = &address.display_name {
            set.extend(tokenize(name));
        }
        set.extend(tokenize(address.email.as_str()));
    }
    set.extend(tokenize(body));
    set
}

/// Scores a message from its tokens' corpus counts, returning a spam
/// probability in `0.0..=1.0`. Tokens never seen in training (or all tokens,
/// when the corpus is empty) carry no weight; if nothing is decisive the result
/// is [`NEUTRAL_SCORE`].
#[must_use]
pub fn score(stats: &[TokenStat], corpus: Corpus) -> f64 {
    let mut probs: Vec<f64> = stats
        .iter()
        .filter_map(|stat| token_prob(*stat, corpus))
        .collect();
    if probs.is_empty() {
        return NEUTRAL_SCORE;
    }
    // Keep the most decisive tokens — those whose probability is furthest from
    // 0.5 — so a flood of near-neutral tokens cannot drown the real signal.
    probs.sort_by(|a, b| {
        (b - 0.5)
            .abs()
            .partial_cmp(&(a - 0.5).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    probs.truncate(SIGNIFICANT_TOKENS);

    // Combine independent probabilities: p = ∏p / (∏p + ∏(1-p)).
    let mut spamminess = 1.0f64;
    let mut hamminess = 1.0f64;
    for prob in probs {
        spamminess *= prob;
        hamminess *= 1.0 - prob;
    }
    let denominator = spamminess + hamminess;
    if denominator == 0.0 {
        NEUTRAL_SCORE
    } else {
        spamminess / denominator
    }
}

/// The spamminess of one token, or `None` when it carries no evidence (an empty
/// corpus, or a token seen in neither class).
fn token_prob(stat: TokenStat, corpus: Corpus) -> Option<f64> {
    if corpus.is_empty() || (stat.ham == 0 && stat.spam == 0) {
        return None;
    }
    let ham_ratio = ratio(HAM_BIAS * f64::from(stat.ham), corpus.ham_messages);
    let spam_ratio = ratio(f64::from(stat.spam), corpus.spam_messages);
    let total = ham_ratio + spam_ratio;
    if total == 0.0 {
        return None;
    }
    Some((spam_ratio / total).clamp(MIN_PROB, MAX_PROB))
}

/// `numerator / messages`, capped at 1.0; `0.0` when the class has no messages
/// (so it contributes nothing rather than dividing by zero).
fn ratio(numerator: f64, messages: u32) -> f64 {
    if messages == 0 {
        0.0
    } else {
        (numerator / f64::from(messages)).min(1.0)
    }
}

/// Reads a server's spam verdict from a raw RFC 5322 header block, honoring the
/// widely used SpamAssassin markers. Returns `Some(true)` when a header marks
/// the message as spam, `Some(false)` when one explicitly clears it, and `None`
/// when no recognized header is present. Only the header block is scanned
/// (parsing stops at the first blank line).
#[must_use]
pub fn server_spam_verdict(headers: &str) -> Option<bool> {
    for line in headers.lines() {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let verdict = match name.trim().to_ascii_lowercase().as_str() {
            "x-spam-flag" => yes_no(value),
            // `X-Spam-Status: Yes, score=…` / `No, score=…`.
            "x-spam-status" => yes_no(value.split([',', ' ']).next().unwrap_or_default()),
            _ => None,
        };
        if verdict.is_some() {
            return verdict;
        }
    }
    None
}

/// Interprets a `Yes`/`No` header value (case-insensitive).
fn yes_no(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if value.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_lowercases_and_keeps_signal_shapes() {
        assert_eq!(
            tokenize("Cheap $$$ Low-Cost!"),
            ["cheap", "$$$", "low-cost"]
        );
        assert_eq!(tokenize("   "), Vec::<String>::new());
        // A run longer than the cap is dropped.
        let long = "a".repeat(MAX_TOKEN_LEN + 1);
        assert!(tokenize(&long).is_empty());
    }

    #[test]
    fn empty_corpus_is_neutral() {
        let stats = [TokenStat { ham: 0, spam: 0 }];
        assert_eq!(score(&stats, Corpus::default()), NEUTRAL_SCORE);
    }

    #[test]
    fn unknown_tokens_do_not_move_the_score() {
        let corpus = Corpus {
            ham_messages: 5,
            spam_messages: 5,
        };
        let stats = [TokenStat { ham: 0, spam: 0 }, TokenStat { ham: 0, spam: 0 }];
        assert_eq!(score(&stats, corpus), NEUTRAL_SCORE);
    }

    #[test]
    fn spammy_tokens_score_high_hammy_tokens_score_low() {
        let corpus = Corpus {
            ham_messages: 10,
            spam_messages: 10,
        };
        let spammy = [TokenStat { ham: 0, spam: 9 }];
        let hammy = [TokenStat { ham: 9, spam: 0 }];
        assert!(score(&spammy, corpus) > 0.9, "spammy token should be high");
        assert!(score(&hammy, corpus) < 0.1, "hammy token should be low");
    }

    #[test]
    fn server_verdict_reads_spamassassin_headers() {
        assert_eq!(
            server_spam_verdict("Subject: hi\nX-Spam-Flag: YES\n\nbody"),
            Some(true)
        );
        assert_eq!(
            server_spam_verdict("X-Spam-Status: No, score=0.1\n\n"),
            Some(false)
        );
        assert_eq!(server_spam_verdict("Subject: hi\n\nX-Spam-Flag: YES"), None);
        assert_eq!(server_spam_verdict("Subject: only\n"), None);
    }
}
