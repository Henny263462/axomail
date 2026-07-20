//! Regression (finding #8): run-on-arrival filtering advances the per-account
//! high-water incrementally, so a mid-batch failure still advances past the
//! messages handled before it — a re-run never re-fires a non-idempotent action
//! (like Forward) on an already-processed message.
//!
//! The failure is injected deterministically: the second arrival candidate's
//! Forward action reads its cached raw from the offline store, and that path is
//! made a *directory*, so the read errors and the action fails — while the first
//! candidate (no such path) forwards cleanly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axomail_storage::{
    ActionKind, ConditionField, ConditionOp, FilterAction, FilterCondition, FilterInput, MatchType,
    Store, raw_message_path,
};
use axomail_sync::{EngineOptions, SyncConfig, SyncEngine, SyncMailbox};

use support::{
    FakeFactory, FakeWorld, Pass, batch, drain_until_terminal, envelope, store_with_mailbox,
    test_config,
};

const UV: u32 = 100;

async fn forward_filter(store: &Store, account: axomail_core::AccountId) {
    store
        .filters()
        .create(&FilterInput {
            account: Some(account),
            name: "forward fwd mail".to_owned(),
            enabled: true,
            match_type: MatchType::All,
            stop_on_match: false,
            conditions: vec![FilterCondition {
                field: ConditionField::Subject,
                op: ConditionOp::Contains,
                value: "fwd".to_owned(),
            }],
            actions: vec![FilterAction {
                kind: ActionKind::Forward,
                param: "boss@work.example".to_owned(),
            }],
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn arrival_advances_high_water_past_a_message_processed_before_a_failure() {
    let raw_dir = tempfile::tempdir().unwrap();
    let raw_root = raw_dir.path();
    let (store, account, inbox) = store_with_mailbox().await;

    forward_filter(&store, account).await;
    // Baseline the arrival high-water to 0 and enable it, so the three arrivals
    // (ids 1..=3, above the mark) are actually filtered rather than baselined.
    store
        .filters()
        .set_arrival_enabled(account, true, 0)
        .await
        .unwrap();

    // Make the SECOND ingested message (local id 2) fail its Forward: its cached
    // raw path is a directory, so `read_raw` errors instead of returning None.
    fs::create_dir_all(raw_message_path(raw_root, 2)).unwrap();

    // Three inbound messages matching the forward filter.
    let world = FakeWorld::new();
    world.set_plan(
        inbox,
        vec![Pass::Batch(batch(
            UV,
            3,
            false,
            vec![
                envelope(1, UV, "fwd me one", false),
                envelope(2, UV, "fwd me two", false),
                envelope(3, UV, "fwd me three", false),
            ],
        ))],
    );

    // Disable the offline body download so it never touches the raw store here.
    let config = SyncConfig {
        offline_download_batch: 0,
        ..test_config()
    };
    let engine = SyncEngine::start_with_options(
        store.clone(),
        FakeFactory::new(world),
        config,
        EngineOptions {
            offline: Arc::new(AtomicBool::new(false)),
            raw_root: Some(raw_root.to_path_buf().into()),
        },
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

    // All three arrived (Forward does not move the message).
    let ids: Vec<i64> = store
        .messages()
        .list_page(inbox, None, 10)
        .await
        .unwrap()
        .items
        .iter()
        .map(|item| item.id.value())
        .collect();
    assert_eq!(ids.len(), 3, "three messages arrived");

    // The high-water advanced to the first message (handled before id 2 failed),
    // NOT stuck at the 0 baseline. Without the incremental advance the whole run
    // aborts and the mark never moves, so a re-run would re-forward id 1.
    let state = store
        .filters()
        .arrival_state(account)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state.last_message_id, 1,
        "high-water advances past the message processed before the failure"
    );

    // Exactly one forward left (id 1); id 2 failed and id 3 was never reached.
    let outbox = store.outbox().list().await.unwrap();
    assert_eq!(
        outbox.len(),
        1,
        "only the pre-failure message was forwarded"
    );
}
