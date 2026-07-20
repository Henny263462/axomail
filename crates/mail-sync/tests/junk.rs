//! End-to-end auto-junk routing: a sync pass ingests a spam-scored message,
//! the worker files it to Junk with the `$Junk` keyword, and the queued server
//! ops drain through an auto-triggered `OpReplay`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use axomail_core::junk::message_tokens;
use axomail_core::{AccountId, Keyword, MailboxName, MailboxRole, RemoteMessageId};
use axomail_storage::{JunkSettings, NewMailbox, Store};
use axomail_sync::{SyncEngine, SyncMailbox};

use support::{
    FakeFactory, FakeWorld, Pass, batch, drain_until_terminal, envelope, store_with_mailbox,
    test_config,
};

const UV: u32 = 100;

/// Polls until `account`'s write-back queue is empty, or panics after a
/// generous timeout.
async fn wait_until_drained(store: &Store, account: AccountId) {
    for _ in 0..600 {
        let counts = store.pending_ops().counts(account).await.unwrap();
        if counts.pending + counts.inflight == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("junk write-back ops were not drained within the timeout");
}

fn spam_tokens() -> BTreeSet<String> {
    message_tokens(Some("cheap viagra"), &[], "")
}

fn ham_tokens() -> BTreeSet<String> {
    message_tokens(Some("project meeting"), &[], "agenda notes")
}

#[tokio::test]
async fn ingest_auto_routes_a_spam_message_to_junk() {
    let (store, account, inbox) = store_with_mailbox().await;
    // A Junk destination the router can move into.
    let junk = store
        .mailboxes()
        .upsert(&NewMailbox {
            account,
            name: MailboxName::new("Junk"),
            remote_name: MailboxName::new("Junk"),
            role: MailboxRole::Junk,
            uid_validity: None,
            uid_next: None,
        })
        .await
        .unwrap()
        .id;

    // Teach the classifier and enable junk control with a permissive threshold.
    // Train enough messages in both classes to clear the auto-file corpus gate.
    for _ in 0..24 {
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
    store
        .junk()
        .set_settings(&JunkSettings {
            enabled: true,
            threshold: 0.5,
            ..JunkSettings::default()
        })
        .await
        .unwrap();

    // One inbound spam-shaped envelope.
    let world = FakeWorld::new();
    world.set_plan(
        inbox,
        vec![Pass::Batch(batch(
            UV,
            1,
            false,
            vec![envelope(1, UV, "cheap viagra", false)],
        ))],
    );

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: inbox,
        })
        .await
        .unwrap();
    drain_until_terminal(&mut events).await;
    wait_until_drained(&store, account).await;

    // Local disposition: the message left the Inbox for Junk and carries $Junk.
    let inbox_items = store
        .messages()
        .list_page(inbox, None, 10)
        .await
        .unwrap()
        .items;
    assert!(
        inbox_items.is_empty(),
        "spam should not remain in the Inbox"
    );
    let junk_items = store
        .messages()
        .list_page(junk, None, 10)
        .await
        .unwrap()
        .items;
    assert_eq!(junk_items.len(), 1, "spam should be filed in Junk");
    let message_id = junk_items[0].id;

    let junk_keyword = Keyword::parse("$Junk").unwrap();
    let tags = store.tags().tags_for_message(message_id).await.unwrap();
    assert!(
        tags.iter().any(|tag| tag.keyword == junk_keyword),
        "the $Junk keyword should be applied locally"
    );
    let score = store.junk().score_of(message_id).await.unwrap().unwrap();
    assert!(score >= 0.5, "the message should have been scored as junk");

    // Server propagation: the write-back drained a $Junk keyword add and a move.
    let flags = world.flag_calls();
    assert!(
        flags
            .iter()
            .any(|(_, delta)| delta.add.keywords.contains(&junk_keyword)),
        "an additive $Junk keyword op should have replayed"
    );
    let moves = world.move_calls();
    assert!(
        moves.iter().any(|(id, target)| matches!(
            id,
            RemoteMessageId::Imap { uid: 1, uid_validity } if *uid_validity == UV
        ) && target.as_str() == "Junk"),
        "a move to Junk should have replayed, saw {moves:?}"
    );
}

#[tokio::test]
async fn disabled_junk_control_leaves_inbound_in_the_inbox() {
    let (store, account, inbox) = store_with_mailbox().await;
    store
        .mailboxes()
        .upsert(&NewMailbox {
            account,
            name: MailboxName::new("Junk"),
            remote_name: MailboxName::new("Junk"),
            role: MailboxRole::Junk,
            uid_validity: None,
            uid_next: None,
        })
        .await
        .unwrap();
    for _ in 0..6 {
        store
            .junk()
            .train(account, &spam_tokens(), true)
            .await
            .unwrap();
    }
    // Junk control stays disabled (default): no scoring, no routing.

    let world = FakeWorld::new();
    world.set_plan(
        inbox,
        vec![Pass::Batch(batch(
            UV,
            1,
            false,
            vec![envelope(1, UV, "cheap viagra", false)],
        ))],
    );
    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: inbox,
        })
        .await
        .unwrap();
    drain_until_terminal(&mut events).await;

    let inbox_items = store
        .messages()
        .list_page(inbox, None, 10)
        .await
        .unwrap()
        .items;
    assert_eq!(
        inbox_items.len(),
        1,
        "message stays in the Inbox when disabled"
    );
    assert!(world.move_calls().is_empty(), "no move op when disabled");
}
