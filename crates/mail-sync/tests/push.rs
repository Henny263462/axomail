//! Tests for the IMAP IDLE [`PushSupervisor`], driven by the scripted
//! [`FakeProvider`] against an in-memory store and a real [`SyncEngine`].
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axomail_core::{AccountId, MailboxId, MailboxName};
use axomail_sync::{PushConfig, PushState, PushSupervisor, PushTarget, SyncEngine, SyncEvent};
use support::{FailKind, FakeFactory, FakeWorld, IdleStep, store_with_mailbox, test_config};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

/// Fast push config: a long renewal (no spurious re-idle) and tiny reconnect
/// backoff so drop/reconnect tests run in real time.
fn push_config() -> PushConfig {
    PushConfig {
        renew_after: Duration::from_secs(60),
        reconnect_base: Duration::from_millis(5),
        reconnect_cap: Duration::from_millis(40),
    }
}

/// The inbox push target for the store built by [`store_with_mailbox`].
fn inbox_target(account: AccountId, inbox: MailboxId) -> PushTarget {
    PushTarget {
        account_id: account,
        mailbox_id: inbox,
        remote_name: MailboxName::new("INBOX"),
    }
}

/// Waits (up to 5s) for the first event matching `pred`.
async fn wait_for<F>(rx: &mut broadcast::Receiver<SyncEvent>, mut pred: F) -> SyncEvent
where
    F: FnMut(&SyncEvent) -> bool,
{
    loop {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Ok(event)) => {
                if pred(&event) {
                    return event;
                }
            }
            Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) => panic!("event channel closed early"),
            Err(_) => panic!("timed out waiting for a matching event"),
        }
    }
}

/// Polls `cond` (up to 5s) until it holds.
async fn wait_until<F: Fn() -> bool>(cond: F) {
    let start = Instant::now();
    while !cond() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "condition was not met in time"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// A push connection-state event for `account`.
fn push_state(event: &SyncEvent, account: AccountId) -> Option<PushState> {
    match event {
        SyncEvent::PushStateChanged {
            account_id, state, ..
        } if *account_id == account => Some(*state),
        _ => None,
    }
}

#[tokio::test]
async fn exists_notification_triggers_a_sync() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    // One push, then the connection goes quiet (parks on the interrupt).
    world.script_idle(vec![IdleStep::Notified]);
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let mut events = engine.subscribe();
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );

    // The connection comes up as a push connection ...
    let state = wait_for(&mut events, |event| push_state(event, account).is_some()).await;
    assert_eq!(push_state(&state, account), Some(PushState::Connected));

    // ... and the EXISTS push enqueues an immediate sync of the inbox.
    let queued = wait_for(
        &mut events,
        |event| matches!(event, SyncEvent::JobQueued { mailbox_id: Some(id), .. } if *id == inbox),
    )
    .await;
    assert!(matches!(queued, SyncEvent::JobQueued { .. }));

    push.shutdown();
}

#[tokio::test]
async fn missing_idle_capability_falls_back_to_polling() {
    let world = FakeWorld::new();
    world.set_idle_supported(false); // server advertises no IDLE
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let mut events = engine.subscribe();
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );

    let state = wait_for(&mut events, |event| push_state(event, account).is_some()).await;
    assert_eq!(
        push_state(&state, account),
        Some(PushState::Polling),
        "no IDLE => poll fallback announced"
    );
    // It never idled (so it never pushed a sync); the poller covers the account.
    assert_eq!(world.idle_calls(), 0);

    push.shutdown();
}

#[tokio::test]
async fn timeout_renews_idle_then_a_push_syncs() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    // First wait renews (TimedOut); the second is a real push.
    world.script_idle(vec![IdleStep::TimedOut, IdleStep::Notified]);
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let mut events = engine.subscribe();
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );

    // The renewal re-issues IDLE (at least two calls) ...
    wait_until(|| world.idle_calls() >= 2).await;
    // ... and the subsequent push still enqueues a sync.
    wait_for(
        &mut events,
        |event| matches!(event, SyncEvent::JobQueued { mailbox_id: Some(id), .. } if *id == inbox),
    )
    .await;

    push.shutdown();
}

#[tokio::test]
async fn dropped_connection_reconnects_with_backoff() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    // Three consecutive drops, then quiet: each drop forces a backed-off
    // reconnect (a fresh connect), so `connect_calls` climbs.
    world.script_idle(vec![
        IdleStep::Fail(FailKind::Temporary),
        IdleStep::Fail(FailKind::Temporary),
        IdleStep::Fail(FailKind::Temporary),
    ]);
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let mut events = engine.subscribe();
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );

    // At least one reconnect announcement, and connect was retried per drop.
    wait_for(&mut events, |event| {
        push_state(event, account) == Some(PushState::Reconnecting)
    })
    .await;
    wait_until(|| world.connect_calls() >= 3).await;

    push.shutdown();
}

#[tokio::test]
async fn toggle_disables_and_re_enables_push() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    // Empty script: the connection parks quietly on the interrupt.
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );
    assert_eq!(push.active_accounts(), vec![account], "starts enabled");
    assert!(push.is_enabled());

    // Disabling stops the task (a clean DONE + disconnect happens in the task).
    push.set_enabled(false);
    assert!(!push.is_enabled());
    assert!(push.active_accounts().is_empty(), "disabled => no tasks");

    // Re-enabling spins the task back up.
    push.set_enabled(true);
    assert_eq!(push.active_accounts(), vec![account], "re-enabled");

    push.shutdown();
    assert!(push.active_accounts().is_empty());
}

/// Regression (finding #20): when an account's push target changes, the
/// replacement task awaits the outgoing connection's teardown before opening its
/// own — the two IDLE sessions never overlap.
#[tokio::test]
async fn restart_awaits_teardown_before_reconnecting() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    // A slow teardown widens the window a too-eager reconnect would overlap in.
    world.set_disconnect_delay(Duration::from_millis(80));
    // Empty idle script: the connection parks quietly on the interrupt.
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        true,
        [inbox_target(account, inbox)],
    );

    // Wait for the first IDLE connection to come up.
    wait_until(|| world.idle_calls() >= 1).await;
    assert_eq!(world.max_open_connections(), 1);

    // Change the account's target (a different remote name): its task restarts.
    let changed = PushTarget {
        account_id: account,
        mailbox_id: inbox,
        remote_name: MailboxName::new("Archive"),
    };
    push.update_targets([changed]);

    // The replacement eventually opens its own connection (a second connect) ...
    wait_until(|| world.connect_calls() >= 2).await;
    // ... but the old and new IDLE sessions never overlapped.
    assert_eq!(
        world.max_open_connections(),
        1,
        "a restart must not hold two IDLE sessions at once"
    );

    push.shutdown();
}

#[tokio::test]
async fn starting_disabled_runs_no_tasks() {
    let world = FakeWorld::new();
    world.set_idle_supported(true);
    let (store, account, inbox) = store_with_mailbox().await;

    let engine = Arc::new(SyncEngine::start(
        store.clone(),
        FakeFactory::new(Arc::clone(&world)),
        test_config(),
    ));
    let push = PushSupervisor::start(
        Arc::clone(&engine),
        store.clone(),
        push_config(),
        false, // disabled at start (e.g. offline or setting off)
        [inbox_target(account, inbox)],
    );

    assert!(push.active_accounts().is_empty());
    // Give any (erroneously spawned) task a chance to connect; it must not.
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        world.connect_calls(),
        0,
        "disabled push opens no connection"
    );

    push.shutdown();
}
