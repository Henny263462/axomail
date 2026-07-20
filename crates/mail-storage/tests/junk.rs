//! Storage-level tests for the junk classifier: training/scoring separation,
//! empty-corpus neutrality, settings round-trip, and ingest-time scoring with
//! auto-junk candidate detection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;

use axomail_core::junk::message_tokens;
use axomail_core::{Address, EmailAddress, MailboxRole, SyncOutcome};
use axomail_storage::{JunkSettings, NewContact, NewContactEmail, Store};

use common::{remote_envelope, sample_account, sample_mailbox, sample_message};

/// Messages trained per class when a test needs the corpus to be large enough
/// to *auto-file* (see [`axomail_core::junk::MIN_AUTOFILE_MESSAGES`]).
const TRAINED: usize = 24;

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

/// Trains `count` messages of each token set so the corpus clears the auto-file
/// gate. Uses the unconditional `train` primitive (a simulated corpus).
async fn train_corpus(
    store: &Store,
    account: axomail_core::AccountId,
    spam: &BTreeSet<String>,
    ham: &BTreeSet<String>,
    count: usize,
) {
    for _ in 0..count {
        store.junk().train(account, spam, true).await.unwrap();
        store.junk().train(account, ham, false).await.unwrap();
    }
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
        mark_read_on_junk: true,
    };
    store.junk().set_settings(&desired).await.unwrap();
    let loaded = store.junk().settings().await.unwrap();
    assert_eq!(loaded, desired);
}

#[tokio::test]
async fn ingest_scores_inbound_and_flags_auto_junk_candidates() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;

    // Teach the classifier that the incoming subject tokens are spam, with a
    // corpus large enough to clear the auto-file gate.
    train_corpus(
        &store,
        account,
        &message_tokens(Some("cheap viagra"), &[], ""),
        &ham_tokens(),
        TRAINED,
    )
    .await;
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

/// Regression (finding #3): a tiny corpus scores a message but never auto-files
/// it — the auto-file gate needs enough training in both classes.
#[tokio::test]
async fn tiny_corpus_scores_but_does_not_autofile() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;

    // A decisive but tiny corpus: tokens seen enough to be confident, but far
    // fewer than MIN_AUTOFILE_MESSAGES messages.
    train_corpus(
        &store,
        account,
        &message_tokens(Some("cheap viagra"), &[], ""),
        &ham_tokens(),
        6,
    )
    .await;
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
    assert!(
        summary.auto_junk_candidates.is_empty(),
        "a tiny corpus must not auto-file"
    );
    // The message is still scored (and the score persisted), decisively high.
    let message = store
        .messages()
        .list_page(inbox, None, 10)
        .await
        .unwrap()
        .items[0]
        .id;
    let score = store.junk().score_of(message).await.unwrap().unwrap();
    assert!(
        score >= 0.5,
        "the message is scored despite not being filed"
    );
}

/// Regression (finding #1): a UIDVALIDITY-change re-ingest re-scores every
/// message but returns no auto-junk candidates, so it can never mass-file the
/// re-fetched inbox (which would destroy user not-junk pins).
#[tokio::test]
async fn uidvalidity_change_reingest_scores_but_does_not_autofile() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;
    train_corpus(
        &store,
        account,
        &message_tokens(Some("cheap viagra"), &[], ""),
        &ham_tokens(),
        TRAINED,
    )
    .await;
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.5,
            ..JunkSettings::default()
        })
        .await
        .unwrap();

    // First sync at UIDVALIDITY 1: an adequate corpus auto-flags the spam.
    let first = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(10),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(10, 1, "cheap viagra")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let first_summary = store.ingest().apply(inbox, &first).await.unwrap();
    assert_eq!(first_summary.auto_junk_candidates.len(), 1);

    // The server reports a new UIDVALIDITY: the inbox is wiped and re-ingested.
    let reingest = SyncOutcome {
        uid_validity: Some(2),
        highest_uid: Some(10),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(10, 2, "cheap viagra")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    let summary = store.ingest().apply(inbox, &reingest).await.unwrap();
    assert!(summary.wiped, "a UIDVALIDITY change wipes the mailbox");
    assert_eq!(summary.inserted, 1);
    assert!(
        summary.auto_junk_candidates.is_empty(),
        "a re-ingest must never auto-file existing mail"
    );
    // But the re-ingested message is still scored.
    let message = store
        .messages()
        .list_page(inbox, None, 10)
        .await
        .unwrap()
        .items[0]
        .id;
    let score = store.junk().score_of(message).await.unwrap().unwrap();
    assert!(score >= 0.5, "the re-ingested message is still scored");
}

/// Regression (finding #4): training is idempotent per (message, class) and a
/// class switch fully reverses the prior counts.
#[tokio::test]
async fn message_training_is_idempotent_and_reversible() {
    let store = Store::open_in_memory().await.unwrap();
    let (account, inbox) = account_with_inbox(&store).await;
    let message = store
        .messages()
        .insert(&sample_message(
            inbox,
            "cheap viagra pills",
            Some(1_700_000_000),
        ))
        .await
        .unwrap()
        .id;
    let tokens = spam_tokens();

    // Two mark-junk clicks on the same message count as one training.
    store
        .junk()
        .train_message(account, message, &tokens, true)
        .await
        .unwrap();
    store
        .junk()
        .train_message(account, message, &tokens, true)
        .await
        .unwrap();
    let corpus = store.junk().corpus(account).await.unwrap();
    assert_eq!(corpus.spam_messages, 1, "double mark-junk is one training");
    assert_eq!(corpus.ham_messages, 0);
    assert_eq!(
        store
            .junk()
            .token_stat(account, "viagra")
            .await
            .unwrap()
            .spam,
        1
    );

    // A not-junk correction fully reverses the spam counts and adds ham.
    store
        .junk()
        .train_message(account, message, &tokens, false)
        .await
        .unwrap();
    let corpus = store.junk().corpus(account).await.unwrap();
    assert_eq!(corpus.spam_messages, 0, "the spam training is reversed");
    assert_eq!(corpus.ham_messages, 1, "and re-trained as ham");
    let stat = store.junk().token_stat(account, "viagra").await.unwrap();
    assert_eq!(stat.spam, 0);
    assert_eq!(stat.ham, 1);
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

    // Train "cheap viagra" as spam (adequate corpus) so the incoming message
    // would otherwise cross the threshold and be filed.
    train_corpus(
        &store,
        account,
        &message_tokens(Some("cheap viagra"), &[], ""),
        &ham_tokens(),
        TRAINED,
    )
    .await;
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
            mark_read_on_junk: false,
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
    train_corpus(
        &store,
        account,
        &spam_tokens(),
        &message_tokens(
            Some("grace hopper"),
            std::slice::from_ref(&hammy_sender()),
            "",
        ),
        TRAINED,
    )
    .await;
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
