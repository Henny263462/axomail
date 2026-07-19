//! Integration tests for the offline operation queue (`pending_ops`):
//! per-message ordering with head-blocking, atomic claiming, park/skip
//! semantics, crash recovery, and FK cascade on account/message delete.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;

use axomail_core::{AccountId, MessageId, RemoteMessageId, SyncOutcome};
use axomail_storage::{NewPendingOp, PendingOp, PendingOpKind, Store};
use common::{remote_envelope, sample_account, sample_mailbox, sample_message};

/// Opens a store with one account and `message_count` local messages, and
/// returns their ids in insertion order.
async fn setup(message_count: usize) -> (Store, AccountId, Vec<MessageId>) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap()
        .id;
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account, "INBOX"))
        .await
        .unwrap()
        .id;
    let mut messages = Vec::new();
    for index in 0..message_count {
        let message = store
            .messages()
            .insert(&sample_message(
                mailbox,
                &format!("m{index}"),
                Some(index as i64),
            ))
            .await
            .unwrap();
        messages.push(message.id);
    }
    (store, account, messages)
}

/// Enqueues an `add_flags` op with an empty JSON payload.
async fn enqueue(store: &Store, account: AccountId, message: MessageId) -> PendingOp {
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: message,
            kind: PendingOpKind::AddFlags,
            payload_json: "{}".to_owned(),
        })
        .await
        .unwrap()
}

fn ids(ops: &[PendingOp]) -> Vec<i64> {
    ops.iter().map(|op| op.id.value()).collect()
}

#[tokio::test]
async fn claim_orders_by_id_and_blocks_later_ops_for_the_same_message() {
    let (store, account, messages) = setup(2).await;
    let repo = store.pending_ops();

    // Two ops on m0 (op1 before op2), one on m1.
    let op1 = enqueue(&store, account, messages[0]).await;
    let op2 = enqueue(&store, account, messages[0]).await;
    let op3 = enqueue(&store, account, messages[1]).await;

    // Only the head of each message is claimed: op1 (m0) and op3 (m1); op2 is
    // blocked behind op1.
    let claimed = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&claimed), vec![op1.id.value(), op3.id.value()]);

    // Nothing else is claimable while op1 is inflight (op2 still blocked).
    assert!(repo.claim_next_batch(account, 10).await.unwrap().is_empty());

    // Completing op1 makes op2 the new head of m0.
    assert!(repo.complete(op1.id).await.unwrap());
    let next = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&next), vec![op2.id.value()]);
}

#[tokio::test]
async fn claim_is_atomic_and_never_double_claims() {
    // Four distinct-message heads, all eligible; two concurrent claimers with
    // a limit of two each must partition them without overlap.
    let (store, account, messages) = setup(4).await;
    let mut expected = HashSet::new();
    for &message in &messages {
        expected.insert(enqueue(&store, account, message).await.id.value());
    }

    let first = store.pending_ops();
    let second = store.pending_ops();
    let (a, b) = tokio::join!(
        first.claim_next_batch(account, 2),
        second.claim_next_batch(account, 2),
    );
    let mut claimed: Vec<i64> = ids(&a.unwrap());
    claimed.extend(ids(&b.unwrap()));

    let unique: HashSet<i64> = claimed.iter().copied().collect();
    assert_eq!(
        unique.len(),
        claimed.len(),
        "no op is claimed twice across concurrent claimers"
    );
    assert_eq!(
        unique, expected,
        "every eligible head is claimed exactly once"
    );
}

#[tokio::test]
async fn fail_parks_the_op_and_requeue_unblocks_the_chain() {
    let (store, account, messages) = setup(1).await;
    let repo = store.pending_ops();
    let op1 = enqueue(&store, account, messages[0]).await;
    let op2 = enqueue(&store, account, messages[0]).await;

    let claimed = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&claimed), vec![op1.id.value()]);

    // Parking op1 blocks its chain: op2 stays unclaimable behind the failed head.
    assert!(repo.fail(op1.id, "boom").await.unwrap());
    assert!(repo.claim_next_batch(account, 10).await.unwrap().is_empty());

    // The parked op is surfaced with its error and bumped attempt count.
    let failed = repo.list_failed(account).await.unwrap();
    assert_eq!(ids(&failed), vec![op1.id.value()]);
    assert_eq!(failed[0].attempts, 1);
    assert_eq!(failed[0].last_error.as_deref(), Some("boom"));

    // Requeue un-parks op1 (no attempt bump); it claims again as the head.
    assert!(repo.requeue(op1.id).await.unwrap());
    let again = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&again), vec![op1.id.value()]);
    assert_eq!(again[0].attempts, 1, "requeue does not bump attempts");

    // Completing op1 finally releases op2.
    assert!(repo.complete(op1.id).await.unwrap());
    let tail = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&tail), vec![op2.id.value()]);
}

#[tokio::test]
async fn a_parked_op_never_wedges_other_messages() {
    let (store, account, messages) = setup(2).await;
    let repo = store.pending_ops();
    let op_m0 = enqueue(&store, account, messages[0]).await;

    // Park the only op on m0.
    repo.claim_next_batch(account, 10).await.unwrap();
    assert!(repo.fail(op_m0.id, "poison").await.unwrap());

    // A brand-new op on a different message is still claimable — the account
    // queue is not wedged by the poison op on m0.
    let op_m1 = enqueue(&store, account, messages[1]).await;
    let claimed = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(ids(&claimed), vec![op_m1.id.value()]);
}

#[tokio::test]
async fn requeue_stale_recovers_crashed_inflight_ops() {
    let (store, account, messages) = setup(2).await;
    let repo = store.pending_ops();
    enqueue(&store, account, messages[0]).await;
    enqueue(&store, account, messages[1]).await;

    // Both heads claimed (now inflight), then a "crash" leaves them stuck.
    let claimed = repo.claim_next_batch(account, 10).await.unwrap();
    assert_eq!(claimed.len(), 2);
    assert!(repo.claim_next_batch(account, 10).await.unwrap().is_empty());

    // Recovery flips inflight rows back to pending so they can be claimed again.
    assert_eq!(repo.requeue_stale().await.unwrap(), 2);
    assert_eq!(repo.claim_next_batch(account, 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn counts_reflect_each_state() {
    let (store, account, messages) = setup(3).await;
    let repo = store.pending_ops();
    let op0 = enqueue(&store, account, messages[0]).await;
    enqueue(&store, account, messages[1]).await; // stays pending, becomes inflight
    enqueue(&store, account, messages[2]).await; // stays pending

    repo.claim_next_batch(account, 2).await.unwrap(); // claims heads of m0, m1
    repo.fail(op0.id, "x").await.unwrap(); // m0 head parked

    let counts = repo.counts(account).await.unwrap();
    assert_eq!(counts.failed, 1);
    assert_eq!(counts.inflight, 1);
    assert_eq!(counts.pending, 1);
}

#[tokio::test]
async fn enqueue_rejects_invalid_json_payload() {
    let (store, account, messages) = setup(1).await;
    let result = store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account,
            message_id: messages[0],
            kind: PendingOpKind::Move,
            payload_json: "not json".to_owned(),
        })
        .await;
    assert!(result.is_err(), "a non-JSON payload is rejected");
}

#[tokio::test]
async fn deleting_the_account_cascades_pending_ops() {
    let (store, account, messages) = setup(1).await;
    enqueue(&store, account, messages[0]).await;
    assert_eq!(
        store.pending_ops().counts(account).await.unwrap().pending,
        1
    );

    assert!(store.accounts().delete(account).await.unwrap());
    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!(
        counts,
        Default::default(),
        "account delete cascades its ops"
    );
}

#[tokio::test]
async fn deleting_a_message_cascades_its_pending_ops() {
    // A message is only ever deleted through the sync-removal path, so drive
    // that: ingest one envelope, queue an op on it, then ingest a removal.
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap()
        .id;
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account, "INBOX"))
        .await
        .unwrap()
        .id;

    let inserted = SyncOutcome {
        uid_validity: Some(100),
        highest_uid: Some(1),
        highest_modseq: None,
        has_more: false,
        new_envelopes: vec![remote_envelope(1, 100, "keep")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox, &inserted).await.unwrap();
    let message = store
        .messages()
        .list_page(mailbox, None, 10)
        .await
        .unwrap()
        .items[0]
        .id;
    enqueue(&store, account, message).await;
    assert_eq!(
        store.pending_ops().counts(account).await.unwrap().pending,
        1
    );

    let removed = SyncOutcome {
        uid_validity: Some(100),
        highest_uid: Some(1),
        highest_modseq: None,
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: vec![RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 100,
        }],
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox, &removed).await.unwrap();

    let counts = store.pending_ops().counts(account).await.unwrap();
    assert_eq!(
        counts,
        Default::default(),
        "message delete cascades its ops"
    );
}
