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
//!
//! Two guards keep a one-sided or tiny corpus from being decisive (a single
//! `mark junk` must never auto-file legitimate mail):
//!
//! - **Per-token gate + smoothing.** A token is only consulted once it has been
//!   seen in at least [`MIN_TOKEN_OCCURRENCES`] trained messages, and even then
//!   its estimate is pulled toward the neutral prior by a Robinson smoothing
//!   term ([`SMOOTHING_STRENGTH`]) so a rare token cannot be maximally decisive;
//!   the estimate is additionally clamped to `[MIN_PROB, MAX_PROB]`.
//! - **Corpus gate for auto-filing.** [`Corpus::can_autofile`] reports whether
//!   the corpus has trained on enough messages in *both* classes
//!   ([`MIN_AUTOFILE_MESSAGES`]) to move mail to Junk automatically. Below that
//!   the caller still scores and persists, but withholds auto-routing.

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
/// Minimum number of trained messages a token must have appeared in before it
/// carries any weight. A token seen only once or twice is noise, so a single
/// `mark junk` cannot make a legitimate word maximally spammy.
const MIN_TOKEN_OCCURRENCES: f64 = 5.0;
/// Robinson smoothing strength: a token's estimate is blended with
/// [`NEUTRAL_SCORE`] as if by this many "prior" observations, so a token with
/// few occurrences stays close to neutral and only a well-seen token approaches
/// its raw ratio.
const SMOOTHING_STRENGTH: f64 = 1.0;
/// Minimum trained messages required **in each class** before auto-filing (a
/// move to Junk) is allowed. Below this the message is still scored and the
/// score persisted, but never auto-routed — so a one-sided or tiny corpus cannot
/// mass-file the inbox. See [`Corpus::can_autofile`].
pub const MIN_AUTOFILE_MESSAGES: u32 = 20;

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

    /// Whether the corpus has trained on enough messages in *both* classes
    /// ([`MIN_AUTOFILE_MESSAGES`] each) to auto-file mail as junk. Below this
    /// threshold the classifier still scores, but the caller must not move mail
    /// to Junk automatically — a one-sided or under-trained corpus is far too
    /// eager and would sweep legitimate mail out of the inbox.
    #[must_use]
    pub fn can_autofile(self) -> bool {
        self.ham_messages >= MIN_AUTOFILE_MESSAGES && self.spam_messages >= MIN_AUTOFILE_MESSAGES
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

/// The spamminess of one token, or `None` when it carries no usable evidence:
/// an empty corpus, or a token seen in fewer than [`MIN_TOKEN_OCCURRENCES`]
/// trained messages (too rare to trust). A usable token's estimate is smoothed
/// toward [`NEUTRAL_SCORE`] and clamped to `[MIN_PROB, MAX_PROB]`, so no single
/// token can be maximally decisive.
fn token_prob(stat: TokenStat, corpus: Corpus) -> Option<f64> {
    // Exact for u32 inputs; the sum never overflows f64's integer range.
    let ham = f64::from(stat.ham);
    let spam = f64::from(stat.spam);
    let occurrences = ham + spam;
    if corpus.is_empty() || occurrences < MIN_TOKEN_OCCURRENCES {
        return None;
    }
    let ham_ratio = ratio(HAM_BIAS * ham, corpus.ham_messages);
    let spam_ratio = ratio(spam, corpus.spam_messages);
    let total = ham_ratio + spam_ratio;
    if total == 0.0 {
        return None;
    }
    let raw = spam_ratio / total;
    // Robinson smoothing: blend the raw ratio with the neutral prior weighted by
    // SMOOTHING_STRENGTH "prior" observations, so a token with few occurrences
    // stays near neutral and only a well-seen token approaches its raw ratio.
    let smoothed = (SMOOTHING_STRENGTH * NEUTRAL_SCORE + occurrences * raw)
        / (SMOOTHING_STRENGTH + occurrences);
    Some(smoothed.clamp(MIN_PROB, MAX_PROB))
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

/// Whether a server's spam header *positively* flags this message as spam,
/// honoring the widely used SpamAssassin markers `X-Spam-Flag: YES` and
/// `X-Spam-Status: Yes, …`. Only the header block is scanned (parsing stops at
/// the first blank line).
///
/// # Trust model (security boundary)
///
/// These headers are attacker-controllable — any sender can stamp an inbound
/// message with `X-Spam-Flag: NO` — so they may only ever *add* a spam signal,
/// never remove one. This function therefore reports **only a positive verdict**:
/// a header that clears the message (`X-Spam-Flag: NO`, `X-Spam-Status: No`) is
/// deliberately ignored and can never force a message to ham past the local
/// Bayesian classifier. The caller must additionally gate the whole check on the
/// user's "trust server junk headers" setting.
#[must_use]
pub fn server_spam_verdict(headers: &str) -> bool {
    for line in headers.lines() {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let flagged = match name.trim().to_ascii_lowercase().as_str() {
            "x-spam-flag" => is_yes(value),
            // `X-Spam-Status: Yes, score=…` / `No, score=…`.
            "x-spam-status" => is_yes(value.split([',', ' ']).next().unwrap_or_default()),
            _ => false,
        };
        if flagged {
            return true;
        }
    }
    false
}

/// Whether a header value is an affirmative `Yes` (case-insensitive).
fn is_yes(value: &str) -> bool {
    value.eq_ignore_ascii_case("yes")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
    fn rare_tokens_are_gated_and_probabilities_clamped() {
        let corpus = Corpus {
            ham_messages: 30,
            spam_messages: 30,
        };
        // A token seen in only four messages is below the occurrence gate, so it
        // carries no weight and the message stays neutral.
        let too_rare = [TokenStat { ham: 0, spam: 4 }];
        assert_eq!(score(&too_rare, corpus), NEUTRAL_SCORE);

        // A one-sided token seen enough times is decisive but never reaches 1.0:
        // it is smoothed toward neutral and clamped to MAX_PROB.
        let one_sided = [TokenStat { ham: 0, spam: 30 }];
        let prob = token_prob(TokenStat { ham: 0, spam: 30 }, corpus).unwrap();
        assert!(prob <= MAX_PROB, "a token probability must be clamped");
        assert!(
            prob >= 0.9,
            "a well-seen spam token should still be decisive"
        );
        assert!(score(&one_sided, corpus) <= MAX_PROB);
    }

    #[test]
    fn corpus_gate_requires_both_classes() {
        assert!(!Corpus::default().can_autofile());
        // Twenty spam but no ham is one-sided: still not allowed to auto-file.
        assert!(
            !Corpus {
                ham_messages: 0,
                spam_messages: 40,
            }
            .can_autofile()
        );
        assert!(
            Corpus {
                ham_messages: MIN_AUTOFILE_MESSAGES,
                spam_messages: MIN_AUTOFILE_MESSAGES,
            }
            .can_autofile()
        );
    }

    #[test]
    fn server_verdict_flags_only_a_positive_marker() {
        // A positive marker is honored.
        assert!(server_spam_verdict("Subject: hi\nX-Spam-Flag: YES\n\nbody"));
        assert!(server_spam_verdict("X-Spam-Status: Yes, score=9.1\n\n"));
        // A NO marker must NOT be reported (it can never force ham). This is the
        // evasion the trust model closes: an attacker-set NO is simply ignored.
        assert!(!server_spam_verdict("X-Spam-Status: No, score=0.1\n\n"));
        assert!(!server_spam_verdict("X-Spam-Flag: NO\n\nbody"));
        // A later positive marker still wins over an earlier NO.
        assert!(server_spam_verdict(
            "X-Spam-Flag: NO\nX-Spam-Status: Yes, score=9\n\n"
        ));
        // Only the header block is scanned; nothing in the body counts.
        assert!(!server_spam_verdict("Subject: hi\n\nX-Spam-Flag: YES"));
        assert!(!server_spam_verdict("Subject: only\n"));
    }
}
