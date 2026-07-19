//! Integration tests for the tag commands: starter-tag seeding on account
//! creation, tag CRUD with derived unique keywords, the per-message picker
//! options and tag/untag mutation (which enqueues a keyword write-back op for
//! IMAP messages only), and the tag-filter listing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MessageFlags, RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::tags::{
    create_tag_impl, delete_tag_impl, list_messages_for_tag_impl, list_tags_impl,
    message_tag_options_impl, set_message_tag_impl, update_tag_impl,
};
use axomail_storage::{NewMessage, Store};
use common::{secret_stores, valid_input};

/// Creates an account (seeding mailboxes + starter tags) and returns the store,
/// account id, and inbox mailbox id.
async fn setup() -> (Store, i64, i64) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = summary.mailboxes.first().unwrap().id;
    (store, summary.id, inbox)
}

/// A NewMessage carrying an IMAP remote identity, so tag/untag enqueues a
/// keyword write-back op.
fn imap_message(mailbox: MailboxId, uid: u32) -> NewMessage {
    NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: 1,
        }),
        subject: Some("Hello".to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(1)),
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: None,
        has_raw: false,
    }
}

async fn insert_imap_message(store: &Store, mailbox: i64, uid: u32) -> i64 {
    store
        .messages()
        .insert(&imap_message(MailboxId::new(mailbox), uid))
        .await
        .unwrap()
        .id
        .value()
}

#[tokio::test]
async fn account_creation_seeds_starter_tags() {
    let (store, account, _inbox) = setup().await;
    let tags = list_tags_impl(&store, account).await.unwrap();
    let names: Vec<&str> = tags.iter().map(|t| t.display_name.as_str()).collect();
    assert_eq!(names, ["Important", "Work", "Personal", "To Do", "Later"]);
    // Keywords are Thunderbird's `$labelN`; every seed carries a color.
    assert_eq!(tags[0].keyword, "$label1");
    assert!(tags.iter().all(|t| t.color.is_some()));
}

#[tokio::test]
async fn create_derives_unique_keywords_and_update_edits() {
    let (store, account, _inbox) = setup().await;
    let first = create_tag_impl(
        &store,
        account,
        "Project".to_owned(),
        Some("#aabbcc".to_owned()),
    )
    .await
    .unwrap();
    assert_eq!(first.keyword, "Project");
    assert_eq!(first.color.as_deref(), Some("#aabbcc"));

    // A second tag with the same name derives a distinct keyword.
    let second = create_tag_impl(&store, account, "Project".to_owned(), None)
        .await
        .unwrap();
    assert_ne!(second.keyword, first.keyword);

    let renamed = update_tag_impl(&store, first.id, "Renamed".to_owned(), None)
        .await
        .unwrap();
    assert_eq!(renamed.display_name, "Renamed");
    assert!(renamed.color.is_none());

    delete_tag_impl(&store, second.id).await.unwrap();
    assert!(
        list_tags_impl(&store, account)
            .await
            .unwrap()
            .iter()
            .all(|t| t.id != second.id)
    );
}

#[tokio::test]
async fn invalid_color_is_rejected() {
    let (store, account, _inbox) = setup().await;
    let result = create_tag_impl(&store, account, "Bad".to_owned(), Some("red".to_owned())).await;
    assert!(result.is_err(), "an invalid color must be rejected");
}

#[tokio::test]
async fn tagging_an_imap_message_links_locally_and_enqueues_write_back() {
    let (store, account, inbox) = setup().await;
    let message = insert_imap_message(&store, inbox, 1).await;
    let tag = list_tags_impl(&store, account).await.unwrap()[0].id;

    set_message_tag_impl(&store, message, tag, true)
        .await
        .unwrap();

    // Local link is reflected in the picker options.
    let options = message_tag_options_impl(&store, message).await.unwrap();
    assert_eq!(options.account_id, account);
    let picked = options.options.iter().find(|o| o.id == tag).unwrap();
    assert!(picked.applied);

    // The filter view now returns the message with its chip.
    let page = list_messages_for_tag_impl(&store, tag).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.items[0].tags.iter().any(|chip| chip.id == tag));

    // An IMAP message enqueues a keyword write-back op.
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account))
        .await
        .unwrap();
    assert_eq!(counts.pending, 1);

    // Untagging enqueues a second (remove) op and clears the local link.
    set_message_tag_impl(&store, message, tag, false)
        .await
        .unwrap();
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account))
        .await
        .unwrap();
    assert_eq!(counts.pending, 2);
    let options = message_tag_options_impl(&store, message).await.unwrap();
    assert!(
        !options
            .options
            .iter()
            .find(|o| o.id == tag)
            .unwrap()
            .applied
    );
}

#[tokio::test]
async fn tagging_a_local_message_does_not_enqueue_write_back() {
    let (store, account, inbox) = setup().await;
    // A local-only message (no remote id) keeps the tag locally with no op.
    let message = store
        .messages()
        .insert(&common::sample_message(
            MailboxId::new(inbox),
            "draft",
            Some(1),
        ))
        .await
        .unwrap()
        .id
        .value();
    let tag = list_tags_impl(&store, account).await.unwrap()[0].id;

    set_message_tag_impl(&store, message, tag, true)
        .await
        .unwrap();

    let options = message_tag_options_impl(&store, message).await.unwrap();
    assert!(
        options
            .options
            .iter()
            .find(|o| o.id == tag)
            .unwrap()
            .applied
    );
    let counts = store
        .pending_ops()
        .counts(AccountId::new(account))
        .await
        .unwrap();
    assert_eq!(counts.pending, 0, "local message enqueues no server write");
}

#[tokio::test]
async fn tagging_rejects_a_cross_account_tag() {
    let (store, account_a, inbox_a) = setup().await;
    // A second account with its own tags.
    let (_concrete, secrets) = secret_stores();
    let mut input = valid_input();
    input.email = "second@example.org".to_owned();
    input.incoming.username = "second@example.org".to_owned();
    input.outgoing.username = "second@example.org".to_owned();
    let account_b = create_account_impl(&store, &secrets, input).await.unwrap();

    let message = insert_imap_message(&store, inbox_a, 1).await;
    let foreign_tag = list_tags_impl(&store, account_b.id).await.unwrap()[0].id;

    let result = set_message_tag_impl(&store, message, foreign_tag, true).await;
    assert!(
        result.is_err(),
        "a tag from another account must be rejected"
    );
    // Sanity: account_a still owns its own tags.
    assert!(!list_tags_impl(&store, account_a).await.unwrap().is_empty());
}
