//! Classifier separation over a small simulated corpus, driving the public
//! `message_tokens` + `score` API end to end.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use axomail_core::junk::{Corpus, TokenStat, message_tokens, score};
use axomail_core::{Address, EmailAddress};

/// A tiny in-test corpus: counts, per distinct token, how many ham and spam
/// messages contained it, and the message totals — exactly what a storage token
/// store would hold.
#[derive(Default)]
struct Trainer {
    corpus: Corpus,
    tokens: std::collections::HashMap<String, TokenStat>,
}

impl Trainer {
    fn train(&mut self, tokens: &BTreeSet<String>, spam: bool) {
        if spam {
            self.corpus.spam_messages += 1;
        } else {
            self.corpus.ham_messages += 1;
        }
        for token in tokens {
            let stat = self.tokens.entry(token.clone()).or_default();
            if spam {
                stat.spam += 1;
            } else {
                stat.ham += 1;
            }
        }
    }

    fn score(&self, tokens: &BTreeSet<String>) -> f64 {
        let stats: Vec<TokenStat> = tokens
            .iter()
            .map(|token| self.tokens.get(token).copied().unwrap_or_default())
            .collect();
        score(&stats, self.corpus)
    }
}

fn body_tokens(body: &str) -> BTreeSet<String> {
    message_tokens(Some("subject"), &[], body)
}

#[test]
fn trained_corpus_separates_ham_from_spam() {
    let mut trainer = Trainer::default();
    for _ in 0..8 {
        trainer.train(
            &body_tokens("project meeting agenda notes schedule team"),
            false,
        );
    }
    for _ in 0..8 {
        trainer.train(
            &body_tokens("viagra cheap pills money winner prize free"),
            true,
        );
    }

    let ham = trainer.score(&body_tokens("team meeting about the project schedule"));
    let spam = trainer.score(&body_tokens("cheap viagra and free pills winner"));
    assert!(ham < 0.2, "ham-like message should score low, got {ham}");
    assert!(
        spam > 0.8,
        "spam-like message should score high, got {spam}"
    );
}

#[test]
fn sender_address_contributes_to_the_token_set() {
    let address = Address {
        display_name: Some("Cheap Deals".to_owned()),
        email: EmailAddress::parse("promo@bargains.example").unwrap(),
    };
    let tokens = message_tokens(
        Some("Save now"),
        std::slice::from_ref(&address),
        "buy today",
    );
    assert!(tokens.contains("cheap"));
    assert!(tokens.contains("promo"));
    assert!(tokens.contains("bargains"));
    assert!(tokens.contains("save"));
}
