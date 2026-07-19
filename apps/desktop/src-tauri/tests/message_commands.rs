//! Integration tests for the message command surface.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{MailError, MailboxId};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::messages::list_messages_impl;
use axomail_storage::Store;
use common::{sample_message, secret_stores, valid_input};

/// Creates an account and returns its inbox mailbox id.
async fn inbox_of_new_account(store: &Store) -> MailboxId {
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(store, &secrets, valid_input())
        .await
        .unwrap();
    MailboxId::new(summary.mailboxes[0].id)
}

#[tokio::test]
async fn list_messages_paginates_newest_first() {
    let store = Store::open_in_memory().await.unwrap();
    let mailbox = inbox_of_new_account(&store).await;

    let mut inserted = Vec::new();
    for (index, date) in [Some(300), Some(200), Some(100), None]
        .into_iter()
        .enumerate()
    {
        let envelope = store
            .messages()
            .insert(&sample_message(mailbox, &format!("m{index}"), date))
            .await
            .unwrap();
        inserted.push(envelope.id.value());
    }
    // date DESC then id DESC, undated last: exactly insertion order here.
    let expected = inserted.clone();

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = list_messages_impl(&store, mailbox.value(), cursor.clone(), 2)
            .await
            .unwrap();
        pages += 1;
        seen.extend(page.items.iter().map(|item| item.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(seen, expected);
    assert_eq!(
        pages, 2,
        "four rows at a page size of two must span two pages"
    );
}

#[tokio::test]
async fn list_messages_rejects_a_malformed_cursor() {
    let store = Store::open_in_memory().await.unwrap();
    let mailbox = inbox_of_new_account(&store).await;

    let error = list_messages_impl(&store, mailbox.value(), Some("not-a-cursor".to_owned()), 10)
        .await
        .unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));
}

// The read/flag write-back commands moved to `commands::writeback`; their
// behaviour (local apply + op enqueue, POP3 local-only) is covered in
// `writeback_commands.rs`.
