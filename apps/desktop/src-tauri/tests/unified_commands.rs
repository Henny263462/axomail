//! Integration tests for `list_unified_inbox`: cross-account, date-ordered
//! paging over every inbox-role mailbox.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{MailboxId, MailboxRole};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::unified::list_unified_inbox_impl;
use axomail_storage::Store;

use common::{sample_message, secret_stores, valid_input};

/// Finds the inbox mailbox id in an account summary.
fn inbox_id(summary: &axomail_desktop_lib::dto::AccountSummaryDto) -> MailboxId {
    let id = summary
        .mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .expect("account has an inbox")
        .id;
    MailboxId::new(id)
}

#[tokio::test]
async fn unified_inbox_pages_newest_first_across_accounts() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();

    let account_a = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account_b = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox_a = inbox_id(&account_a);
    let inbox_b = inbox_id(&account_b);

    // Dates: A=3000 (newest), B=2000, A=1000 (oldest).
    store
        .messages()
        .insert(&sample_message(inbox_a, "Newest from A", Some(3_000)))
        .await
        .unwrap();
    store
        .messages()
        .insert(&sample_message(inbox_b, "Middle from B", Some(2_000)))
        .await
        .unwrap();
    store
        .messages()
        .insert(&sample_message(inbox_a, "Oldest from A", Some(1_000)))
        .await
        .unwrap();

    // First page of two, newest first, spanning both accounts.
    let page1 = list_unified_inbox_impl(&store, None, 2).await.unwrap();
    assert_eq!(page1.items.len(), 2);
    assert_eq!(
        page1.items[0].item.subject.as_deref(),
        Some("Newest from A")
    );
    assert_eq!(page1.items[0].account_id, account_a.id);
    assert_eq!(
        page1.items[1].item.subject.as_deref(),
        Some("Middle from B")
    );
    assert_eq!(page1.items[1].account_id, account_b.id);
    let cursor = page1.next_cursor.expect("a second page follows");

    // Second page carries the remaining, oldest message and ends the stream.
    let page2 = list_unified_inbox_impl(&store, Some(cursor), 2)
        .await
        .unwrap();
    assert_eq!(page2.items.len(), 1);
    assert_eq!(
        page2.items[0].item.subject.as_deref(),
        Some("Oldest from A")
    );
    assert_eq!(page2.items[0].account_id, account_a.id);
    assert!(page2.next_cursor.is_none(), "no further pages");
}

#[tokio::test]
async fn unified_inbox_is_empty_without_messages() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let page = list_unified_inbox_impl(&store, None, 10).await.unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
}
