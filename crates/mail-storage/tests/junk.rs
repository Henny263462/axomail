//! Storage-level tests for the junk classifier: training/scoring separation,
//! empty-corpus neutrality, settings round-trip, and ingest-time scoring with
//! auto-junk candidate detection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;

use axomail_core::junk::message_tokens;
use axomail_core::{Address, EmailAddress, MailboxRole, SyncOutcome};
use axomail_storage::{JunkSettings, NewContact, NewContactEmail, Store};

use common::{remote_envelope, sample_account, sample_mailbox};

/// Tokens of a spam-shaped message, as ingest/commands would build them.
fn spam_tokens() -> BTreeSet<String> {
    message_tokens(Some("cheap viagra pills"), &[], "free money winner prize")
}

/// Tokens of a ham-shaped message.
fn ham_tokens() -> BTreeSet<String> {
    message_tokens(
        Some("project meeting notes"),
        &[],
        "agenda schedule team review",
    )
}

async fn account_with_inbox(store: &Store) -> (axomail_core::AccountId, axomail_core::MailboxId) {
    let account = store
        .accounts()
        .insert(&sample_account("Junk", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let inbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "Inbox"))
        .await
        .unwrap();
    assert_eq!(inbox.role, MailboxRole::Inbox);
    (account.id, inbox.id)
}

#[tokio::test]
async fn training_separates_spam_from_ham() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, _) = account_with_inbox(&store).await;

    for _ in 0..6 {
        store
            .junk()
            .train(account, &spam_tokens(), true)
            .await
            .unwrap();
        store
            .junk()
            .train(account, &ham_tokens(), false)
            .await
            .unwrap();
    }

    let spam_score = store.junk().score(account, &spam_tokens()).await.unwrap();
    let ham_score = store.junk().score(account, &ham_tokens()).await.unwrap();
    assert!(spam_score > 0.8, "spam should score high, got {spam_score}");
    assert!(ham_score < 0.2, "ham should score low, got {ham_score}");

    let corpus = store.junk().corpus(account).await.unwrap();
    assert_eq!(corpus.spam_messages, 6);
    assert_eq!(corpus.ham_messages, 6);
    let stat = store.junk().token_stat(account, "viagra").await.unwrap();
    assert_eq!(stat.spam, 6);
    assert_eq!(stat.ham, 0);
}

#[tokio::test]
async fn empty_corpus_scores_neutral() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, _) = account_with_inbox(&store).await;
    let score = store.junk().score(account, &spam_tokens()).await.unwrap();
    assert!(
        (score - 0.5).abs() < f64::EPSILON,
        "no data must be neutral"
    );
}

#[tokio::test]
async fn settings_round_trip() {
    let store = Store::open_in_memory().await.unwrap();
    // Defaults: disabled, opt-in.
    assert!(!store.junk().settings().await.unwrap().enabled);
    let desired = JunkSettings {
        enabled: true,
        threshold: 0.75,
        trust_server_headers: false,
        whitelist_contacts: false,
    };
    store.junk().set_settings(&desired).await.unwrap();
    let loaded = store.junk().settings().await.unwrap();
    assert_eq!(loaded, desired);
}

#[tokio::test]
async fn ingest_scores_inbound_and_flags_auto_junk_candidates() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;

    // Teach the classifier that the incoming subject tokens are spam.
    for _ in 0..6 {
        store
            .junk()
            .train(
                account,
                &message_tokens(Some("cheap viagra"), &[], ""),
                true,
            )
            .await
            .unwrap();
        store
            .junk()
            .train(account, &ham_tokens(), false)
            .await
            .unwrap();
    }
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.5,
            ..JunkSettings::default()
        })
        .await
        .unwrap();

    let outcome = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(10),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(10, 1, "cheap viagra")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let summary = store.ingest().apply(inbox, &outcome).await.unwrap();

    assert_eq!(summary.inserted, 1);
    assert_eq!(
        summary.auto_junk_candidates.len(),
        1,
        "spam should be flagged"
    );
    let junked = summary.auto_junk_candidates[0];
    let stored = store.junk().score_of(junked).await.unwrap().unwrap();
    assert!(stored >= 0.5, "the stored score must cross the threshold");
}

#[tokio::test]
async fn ingest_does_not_score_non_inbox_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Junk", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let sent = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "Sent"))
        .await
        .unwrap();
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.0,
            ..JunkSettings::default()
        })
        .await
        .unwrap();

    let outcome = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(5),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(5, 1, "anything")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let summary = store.ingest().apply(sent.id, &outcome).await.unwrap();
    assert!(
        summary.auto_junk_candidates.is_empty(),
        "Sent mail is never junk-scored"
    );
}

#[tokio::test]
async fn address_book_membership_drives_the_whitelist() {
    let store = Store::open_in_memory().await.unwrap();
    let (_account, _) = account_with_inbox(&store).await;
    // Save the sender into the (global) address book.
    store
        .contacts()
        .create(&NewContact {
            display_name: "Grace Hopper".to_owned(),
            emails: vec![NewContactEmail::plain(
                EmailAddress::parse("Grace@Example.org").unwrap(),
            )],
            note: String::new(),
        })
        .await
        .unwrap();
    // A saved contact is whitelisted (case-insensitively); a stranger is not.
    assert!(
        store
            .junk()
            .is_whitelisted_sender("grace@example.org")
            .await
            .unwrap(),
        "a saved contact address must be whitelisted"
    );
    assert!(
        !store
            .junk()
            .is_whitelisted_sender("spam@example.org")
            .await
            .unwrap(),
        "a stranger must not be whitelisted"
    );
}

#[tokio::test]
async fn whitelisted_sender_is_not_an_auto_junk_candidate() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;

    // Train "cheap viagra" as spam so the incoming message crosses threshold.
    for _ in 0..6 {
        store
            .junk()
            .train(account, &message_tokens(Some("cheap viagra"), &[], ""), true)
            .await
            .unwrap();
        store
            .junk()
            .train(account, &ham_tokens(), false)
            .await
            .unwrap();
    }
    // Save the sender (grace@example.org, the `remote_envelope` fixture's From)
    // in the address book and turn the contact whitelist on.
    store
        .contacts()
        .create(&NewContact {
            display_name: "Grace Hopper".to_owned(),
            emails: vec![NewContactEmail::plain(
                EmailAddress::parse("grace@example.org").unwrap(),
            )],
            note: String::new(),
        })
        .await
        .unwrap();
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.5,
            trust_server_headers: false,
            whitelist_contacts: true,
        })
        .await
        .unwrap();

    let outcome = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(10),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(10, 1, "cheap viagra")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let summary = store.ingest().apply(inbox, &outcome).await.unwrap();

    // The message is still scored (and stored), but the whitelisted sender keeps
    // it off the auto-junk list.
    assert_eq!(summary.inserted, 1);
    assert!(
        summary.auto_junk_candidates.is_empty(),
        "a message from a saved contact must not be auto-filed as junk"
    );
}

/// Sanity: a scored ham message stays below the threshold and is not flagged.
#[tokio::test]
async fn ham_below_threshold_is_scored_but_not_flagged() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;
    for _ in 0..6 {
        store
            .junk()
            .train(account, &spam_tokens(), true)
            .await
            .unwrap();
        store
            .junk()
            .train(
                account,
                &message_tokens(
                    Some("grace hopper"),
                    std::slice::from_ref(&hammy_sender()),
                    "",
                ),
                false,
            )
            .await
            .unwrap();
    }
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.9,
            ..JunkSettings::default()
        })
        .await
        .unwrap();

    let outcome = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(3),
        highest_modseq: None,
        has_more: false,
        // remote_envelope's sender is grace@example.org with subject as given.
        new_envelopes: vec![remote_envelope(3, 1, "grace hopper")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let summary = store.ingest().apply(inbox, &outcome).await.unwrap();
    assert_eq!(summary.inserted, 1);
    assert!(
        summary.auto_junk_candidates.is_empty(),
        "a ham-scored message must not be auto-junked"
    );
}

fn hammy_sender() -> Address {
    Address {
        display_name: Some("Grace Hopper".to_owned()),
        email: EmailAddress::parse("grace@example.org").unwrap(),
    }
}
