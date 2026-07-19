//! End-to-end tests for the `OpReplay` write-back executor, driven by the
//! scripted `FakeProvider` against an in-memory store.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use axomail_core::{
    AccountId, Address, EmailAddress, Keyword, MailboxId, MailboxName, MessageFlags, MessageId,
    RemoteMessageId,
};
use axomail_storage::{NewMessage, NewPendingOp, PendingOpKind, Store};
use axomail_sync::{SyncEngine, SyncEvent, WorkRequest, flagged_op, keyword_op, seen_op};
use support::{
    FailKind, FakeFactory, FakeProvider, FakeWorld, drain_until_terminal, store_with_mailbox,
    test_config,
};

/// Polls until `account`'s queue has no `pending`/`inflight` ops, or panics
/// after a generous timeout. Used to observe drains that recovery or the
/// drain-window successor trigger without an explicit command to await.
async fn wait_until_drained(store: &Store, account: AccountId) {
    for _ in 0..300 {
        let counts = store.pending_ops().counts(account).await.unwrap();
        if counts.pending + counts.inflight == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("pending ops were not drained within the timeout");
}

const UV: u32 = 100;

/// Inserts a synced (remote-backed) message and returns its local id.
async fn insert_remote(store: &Store, mailbox: MailboxId, uid: u32) -> MessageId {
    let message = NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: UV,
        }),
        subject: Some(format!("m{uid}")),
        from: vec![Address {
            display_name: None,
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: None,
        flags: MessageFlags::default(),
        size_bytes: Some(10),
        preview: None,
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id
}

/// Enqueues a pending op built from a `(kind, payload)` pair.
async fn enqueue(
    store: &Store,
    account: AccountId,
    message: MessageId,
    op: (PendingOpKind, String),
) {
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: message,
            kind: op.0,
            payload_json: op.1,
        })
        .await
        .unwrap();
}

/// Runs one `OpReplay` job for `account` to a terminal state.
async fn run_replay(
    engine: &SyncEngine<FakeProvider, FakeFactory>,
    account: AccountId,
) -> Vec<SyncEvent> {
    let mut events = engine.subscribe();
    engine
        .enqueue_work(WorkRequest::OpReplay {
            account_id: account,
        })
        .await
        .unwrap();
    drain_until_terminal(&mut events).await
}

#[tokio::test]
async fn seen_op_replays_as_an_additive_store_then_completes() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = insert_remote(&store, mailbox, 1).await;
    enqueue(&store, account, message, seen_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let events = run_replay(&engine, account).await;

    // The mailbox was selected and an additive +FLAGS \Seen store was issued.
    assert_eq!(world.select_calls(), vec![MailboxName::new("INBOX")]);
    let flags = world.flag_calls();
    assert_eq!(flags.len(), 1);
    assert!(flags[0].1.add.system.seen);
    assert!(flags[0].1.remove.is_empty());

    // The op was completed (removed) and a summary event reported it.
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (0, 0, 0));
    assert!(events.iter().any(|event| matches!(
        event,
        SyncEvent::OpsReplayed {
            replayed: 1,
            failed: 0,
            ..
        }
    )));
}

#[tokio::test]
async fn a_tag_keyword_op_replays_as_an_additive_keyword_store() {
    // Round-trip proof for the folded keyword executor: the `keyword_op`
    // builder is exactly what the tag/untag command (`set_message_tag`)
    // enqueues, and the single OpReplay executor drains it into an additive
    // `+FLAGS (keyword)` STORE that never clobbers other server keywords.
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = insert_remote(&store, mailbox, 1).await;
    let junk = Keyword::parse("$Junk").unwrap();

    // Tag (add) then untag (remove) the same keyword on one message.
    enqueue(&store, account, message, keyword_op(junk.clone(), true)).await;
    enqueue(&store, account, message, keyword_op(junk.clone(), false)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    run_replay(&engine, account).await;

    let flags = world.flag_calls();
    assert_eq!(flags.len(), 2, "both keyword ops replayed");
    // Add side: +FLAGS ($Junk), nothing removed.
    assert!(flags[0].1.add.keywords.contains(&junk));
    assert!(flags[0].1.add.system.is_empty());
    assert!(flags[0].1.remove.is_empty());
    // Remove side: -FLAGS ($Junk), nothing added.
    assert!(flags[1].1.remove.keywords.contains(&junk));
    assert!(flags[1].1.add.is_empty());

    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (0, 0, 0));
}

#[tokio::test]
async fn ops_for_one_message_replay_in_enqueue_order() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = insert_remote(&store, mailbox, 1).await;
    // Two ops on the same message: mark seen, then star.
    enqueue(&store, account, message, seen_op(true)).await;
    enqueue(&store, account, message, flagged_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    run_replay(&engine, account).await;

    let flags = world.flag_calls();
    assert_eq!(flags.len(), 2, "both ops replayed");
    assert!(flags[0].1.add.system.seen, "seen op first");
    assert!(flags[1].1.add.system.flagged, "flag op second");
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!(counts.pending + counts.inflight + counts.failed, 0);
}

#[tokio::test]
async fn a_poison_op_parks_without_wedging_other_messages() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let poison = insert_remote(&store, mailbox, 1).await;
    let healthy = insert_remote(&store, mailbox, 2).await;
    // A permanent failure targets only message 1 (UID 1).
    world.poison_uid(1, FailKind::Permanent);
    enqueue(&store, account, poison, seen_op(true)).await;
    enqueue(&store, account, healthy, seen_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let events = run_replay(&engine, account).await;

    // The healthy message's op still replayed and completed.
    let flags = world.flag_calls();
    assert_eq!(flags.len(), 1);
    assert_eq!(
        flags[0].0,
        RemoteMessageId::Imap {
            uid: 2,
            uid_validity: UV
        }
    );

    // The poison op is parked (failed), not lost and not blocking the account.
    let failed = store.pending_ops().list_failed(account).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].message_id, poison);
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (0, 0, 1));

    // The job still succeeds (a parked op is not a job failure) and reports the
    // one replay plus one parked op.
    assert!(events.iter().any(|event| matches!(
        event,
        SyncEvent::OpsReplayed {
            replayed: 1,
            failed: 1,
            ..
        }
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SyncEvent::JobFinished { .. }))
    );
}

#[tokio::test]
async fn a_transient_op_is_returned_to_pending_for_a_later_run() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = insert_remote(&store, mailbox, 1).await;
    world.poison_uid(1, FailKind::Temporary);
    enqueue(&store, account, message, seen_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    run_replay(&engine, account).await;

    // A transient failure never parks the op: it is back to pending, its
    // attempt count bumped, ready for the next replay run.
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (1, 0, 0));
}

#[tokio::test]
async fn recovery_drains_pending_ops_without_a_command() {
    // A pending op is in the store, but nothing has enqueued an OpReplay job
    // (e.g. it was written just before a crash). Merely starting the engine must
    // drain it, via startup recovery — no interactive command required.
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let message = insert_remote(&store, mailbox, 1).await;
    enqueue(&store, account, message, seen_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    wait_until_drained(&store, account).await;

    // The recovered op reached the server and was completed — no command ran.
    assert_eq!(world.flag_calls().len(), 1, "the recovered op replayed");
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (0, 0, 0));
    drop(engine);
}

#[tokio::test]
async fn a_drain_window_op_is_not_stranded() {
    // An op that arrives in the drain-completion window — after the running
    // replay's final empty claim but before it transitions terminal — would be
    // deduped away by the command path (an OpReplay is still "running"). The
    // finishing job must enqueue a successor so the op is not stranded.
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let m1 = insert_remote(&store, mailbox, 1).await;
    let m2 = insert_remote(&store, mailbox, 2).await;
    enqueue(&store, account, m1, seen_op(true)).await;

    // The window op: enqueued when the first drain disconnects (after its final
    // empty claim). Without the successor mechanism it stays pending forever.
    world.inject_op_on_disconnect(
        store.clone(),
        NewPendingOp {
            account_id: account,
            message_id: m2,
            kind: PendingOpKind::AddFlags,
            payload_json: seen_op(true).1,
        },
    );

    // Recovery kicks off the first drain (m1); its disconnect injects m2's op;
    // the drain-window fix enqueues a successor that drains it.
    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    wait_until_drained(&store, account).await;

    assert_eq!(
        world.flag_calls().len(),
        2,
        "both the original and the drain-window op replayed"
    );
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!((counts.pending, counts.inflight, counts.failed), (0, 0, 0));
    drop(engine);
}

#[tokio::test]
async fn a_local_only_message_op_is_a_silent_success() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    // A message with no remote identity (e.g. a local draft copy).
    let local = NewMessage {
        mailbox,
        remote_id: None,
        subject: Some("draft".to_owned()),
        from: vec![],
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: None,
        flags: MessageFlags::default(),
        size_bytes: None,
        preview: None,
        has_raw: false,
    };
    let id = store.messages().insert(&local).await.unwrap().id;
    enqueue(&store, account, id, seen_op(true)).await;

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    run_replay(&engine, account).await;

    // Nothing was sent to the server, and the op was completed (removed).
    assert!(world.flag_calls().is_empty());
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!(counts.pending + counts.inflight + counts.failed, 0);
}
