//! Integration tests for `search_messages`: the FTS index joined with
//! envelope and account data through the existing repositories.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::MailboxId;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::search::search_messages_impl;
use axomail_storage::Store;

use common::{sample_message, secret_stores, valid_input};

/// Finds the inbox mailbox id in an account summary.
fn inbox_id(summary: &axomail_desktop_lib::dto::AccountSummaryDto) -> MailboxId {
    let id = summary
        .mailboxes
        .iter()
        .find(|mailbox| mailbox.role == axomail_core::MailboxRole::Inbox)
        .expect("account has an inbox")
        .id;
    MailboxId::new(id)
}

#[tokio::test]
async fn search_returns_hits_joined_with_envelope_and_account() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = inbox_id(&summary);

    store
        .messages()
        .insert(&sample_message(inbox, "Quarterly Report", Some(2_000)))
        .await
        .unwrap();
    store
        .messages()
        .insert(&sample_message(inbox, "Lunch plans", Some(1_000)))
        .await
        .unwrap();

    let results = search_messages_impl(&store, "quarterly".to_owned(), 10)
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "only the quarterly report matches");
    let hit = &results[0];
    assert_eq!(hit.account_id, summary.id);
    assert_eq!(hit.mailbox_id, inbox.value());
    assert_eq!(hit.subject.as_deref(), Some("Quarterly Report"));
    assert_eq!(hit.from_name, "Ada Lovelace");
    assert_eq!(hit.from_address, "ada@example.org");
    assert_eq!(hit.date_unix, Some(2_000));
    assert!(!hit.snippet.is_empty(), "a snippet is always produced");
}

#[tokio::test]
async fn search_matches_sender_and_ranks_multiple_hits() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = inbox_id(&summary);

    for (subject, date) in [("Report one", 3_000), ("Report two", 2_000)] {
        store
            .messages()
            .insert(&sample_message(inbox, subject, Some(date)))
            .await
            .unwrap();
    }

    // Both messages share the sender "Ada Lovelace"; searching the name hits
    // both through the indexed correspondents.
    let by_sender = search_messages_impl(&store, "lovelace".to_owned(), 10)
        .await
        .unwrap();
    assert_eq!(by_sender.len(), 2);

    // The limit is respected.
    let limited = search_messages_impl(&store, "report".to_owned(), 1)
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
}

#[tokio::test]
async fn tokenless_query_yields_no_hits() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = inbox_id(&summary);
    store
        .messages()
        .insert(&sample_message(inbox, "Anything", Some(1)))
        .await
        .unwrap();

    let results = search_messages_impl(&store, "   ".to_owned(), 10)
        .await
        .unwrap();
    assert!(results.is_empty(), "a token-less query matches nothing");
}
