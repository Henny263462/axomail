//! Engine-level tests for the E1 incremental-sync wiring: the QRESYNC
//! present-set reconcile cadence, and op-replay/sync coherence on one mailbox.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axomail_core::{
    Address, EmailAddress, MailboxId, MessageFlags, MessageId, RemoteMessageId, SyncOutcome,
};
use axomail_storage::{NewMessage, NewPendingOp, PendingOpKind, Store};
use axomail_sync::{SyncConfig, SyncEngine, SyncMailbox, WorkRequest, seen_op};
use support::{
    FakeFactory, FakeProvider, FakeWorld, Pass, drain_until_terminal, message_count,
    store_with_mailbox, test_config,
};

const UV: u32 = 100;

/// Inserts a synced (remote-backed) message with the given seen flag.
async fn insert_remote(store: &Store, mailbox: MailboxId, uid: u32, seen: bool) -> MessageId {
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
        flags: MessageFlags {
            seen,
            ..MessageFlags::default()
        },
        size_bytes: Some(10),
        preview: None,
        has_raw: false,
    };
    store.messages().insert(&message).await.unwrap().id
}

/// An IMAP remote id at [`UV`].
fn imap(uid: u32) -> RemoteMessageId {
    RemoteMessageId::Imap {
        uid,
        uid_validity: UV,
    }
}

/// Runs one `MailboxSync` job to a terminal state.
async fn run_sync(
    engine: &SyncEngine<FakeProvider, FakeFactory>,
    account: axomail_core::AccountId,
    mailbox: MailboxId,
) {
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    drain_until_terminal(&mut events).await;
}

#[tokio::test]
async fn reconcile_cadence_reconciles_on_first_pass_then_every_nth() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    // Reconcile the present set every 3rd mailbox-sync job.
    let config = SyncConfig {
        full_reconcile_every: 3,
        ..test_config()
    };
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world.clone()), config);

    // Each job runs one (unscripted, no-op) pass, recording its full_reconcile
    // flag. The first job reconciles (empty counter), then every third.
    for _ in 0..7 {
        run_sync(&engine, account, mailbox).await;
    }

    assert_eq!(
        world.reconcile_flags(mailbox),
        vec![true, false, false, true, false, false, true],
        "reconcile on job 1, 4, 7 (cadence 3, first pass always)"
    );
}

#[tokio::test]
async fn op_then_sync_on_same_mailbox_is_coherent() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    // Two synced messages; uid 2 will be expunged on the server (e.g. by a
    // move our own write-back performed).
    let m1 = insert_remote(&store, mailbox, 1, false).await;
    let _m2 = insert_remote(&store, mailbox, 2, false).await;

    // A write-back op marks uid 1 seen on the server.
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: m1,
            kind: PendingOpKind::AddFlags,
            payload_json: seen_op(true).1,
        })
        .await
        .unwrap();

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );

    // 1) Replay the op: it reaches the server as an additive +FLAGS \Seen.
    let mut events = engine.subscribe();
    engine
        .enqueue_work(WorkRequest::OpReplay {
            account_id: account,
        })
        .await
        .unwrap();
    drain_until_terminal(&mut events).await;
    let flags = world.flag_calls();
    assert_eq!(flags.len(), 1);
    assert!(flags[0].1.add.system.seen, "seen op replayed to the server");

    // 2) A subsequent sync observes the server truth: uid 1 is seen and still
    //    present, uid 2 has vanished. The present set names only uid 1.
    world.set_plan(
        mailbox,
        vec![Pass::Batch(SyncOutcome {
            uid_validity: Some(UV),
            highest_uid: Some(2),
            highest_modseq: Some(50),
            has_more: false,
            new_envelopes: Vec::new(),
            flag_updates: vec![(
                imap(1),
                MessageFlags {
                    seen: true,
                    ..MessageFlags::default()
                },
            )],
            removed: Vec::new(),
            present_remote_ids: Some(vec![imap(1)]),
        })],
    );
    run_sync(&engine, account, mailbox).await;

    // uid 1 kept and coherently seen; uid 2 removed, not resurrected, not
    // double-counted.
    assert_eq!(message_count(&store, mailbox).await, 1);
    let items = store
        .messages()
        .list_page(mailbox, None, 10)
        .await
        .unwrap()
        .items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].subject.as_deref(), Some("m1"));
    assert!(
        items[0].flags.seen,
        "the op's seen change survives the sync"
    );

    // 3) A further sync (no scripted changes) must not resurrect uid 2.
    run_sync(&engine, account, mailbox).await;
    assert_eq!(message_count(&store, mailbox).await, 1);
}
