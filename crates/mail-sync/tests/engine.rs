//! End-to-end tests for the synchronization engine, driven by a scripted,
//! instrumented provider against an in-memory store.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use axomail_core::{MessageFlags, RemoteMessageId};
use axomail_sync::{
    JobState, SyncConfig, SyncEngine, SyncErrorClass, SyncEvent, SyncJobId, SyncMailbox,
};
use support::{
    FailKind, FakeFactory, FakeWorld, Pass, batch, changes, drain_until_terminal, envelope,
    message_count, queued_job_id, store_with_mailbox, store_with_two_mailboxes, test_config,
};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

const UV: u32 = 100;

/// Subject strings of the messages currently stored in a mailbox.
async fn stored_subjects(
    store: &axomail_storage::Store,
    mailbox: axomail_core::MailboxId,
) -> Vec<String> {
    let mut subjects: Vec<String> = store
        .messages()
        .list_page(mailbox, None, 1000)
        .await
        .unwrap()
        .items
        .into_iter()
        .filter_map(|envelope| envelope.subject)
        .collect();
    subjects.sort();
    subjects
}

#[tokio::test]
async fn initial_sync_ingests_envelopes_recipients_and_state() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![Pass::Batch(batch(
            UV,
            2,
            false,
            vec![
                envelope(1, UV, "first", false),
                envelope(2, UV, "second", false),
            ],
        ))],
    );

    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    // Envelopes and their recipients are persisted.
    let page = store.messages().list_page(mailbox, None, 10).await.unwrap();
    assert_eq!(page.items.len(), 2);
    let message = page
        .items
        .iter()
        .find(|item| item.subject.as_deref() == Some("first"))
        .expect("first message present");
    assert_eq!(message.from.len(), 1);
    assert_eq!(
        message.from[0].display_name.as_deref(),
        Some("Ada Lovelace")
    );
    assert_eq!(message.to[0].email.as_str(), "klaus@theferbers.eu");

    // Sync state advanced.
    let state = store.sync_state().get(mailbox).await.unwrap();
    assert_eq!(state.uid_validity, Some(UV));
    assert_eq!(state.highest_uid, Some(2));

    // Job recorded as succeeded and the summary is right.
    let job_id = queued_job_id(&collected);
    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Succeeded);
    match collected.last().unwrap() {
        SyncEvent::JobFinished { summary, .. } => {
            assert_eq!(summary.new_messages, 2);
            assert_eq!(summary.batches, 1);
        }
        other => panic!("expected JobFinished, got {other:?}"),
    }
}

#[tokio::test]
async fn incremental_pass_appends_and_advances_highest_uid() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(
                UV,
                2,
                false,
                vec![envelope(1, UV, "m1", false), envelope(2, UV, "m2", false)],
            )),
            Pass::Batch(batch(UV, 3, false, vec![envelope(3, UV, "m3", false)])),
        ],
    );
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();

    run_one(&engine, &mut events, account, mailbox).await;
    assert_eq!(message_count(&store, mailbox).await, 2);
    assert_eq!(
        store.sync_state().get(mailbox).await.unwrap().highest_uid,
        Some(2)
    );

    run_one(&engine, &mut events, account, mailbox).await;
    assert_eq!(message_count(&store, mailbox).await, 3);
    assert_eq!(
        store.sync_state().get(mailbox).await.unwrap().highest_uid,
        Some(3)
    );
}

#[tokio::test]
async fn has_more_loops_until_exhausted_persisting_each_batch() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(UV, 1, true, vec![envelope(1, UV, "a1", false)])),
            Pass::Batch(batch(UV, 2, true, vec![envelope(2, UV, "a2", false)])),
            Pass::Batch(batch(UV, 3, false, vec![envelope(3, UV, "a3", false)])),
        ],
    );
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    let batches = collected
        .iter()
        .filter(|event| matches!(event, SyncEvent::BatchIngested { .. }))
        .count();
    assert_eq!(batches, 3, "each pass persists its own batch");
    assert_eq!(message_count(&store, mailbox).await, 3);
    assert_eq!(
        store.sync_state().get(mailbox).await.unwrap().highest_uid,
        Some(3)
    );
    match collected.last().unwrap() {
        SyncEvent::JobFinished { summary, .. } => {
            assert_eq!(summary.batches, 3);
            assert_eq!(summary.new_messages, 3);
        }
        other => panic!("expected JobFinished, got {other:?}"),
    }
}

#[tokio::test]
async fn uid_validity_change_wipes_and_refetches() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(
                100,
                2,
                false,
                vec![
                    envelope(1, 100, "old-1", false),
                    envelope(2, 100, "old-2", false),
                ],
            )),
            Pass::Batch(batch(
                200,
                12,
                false,
                vec![
                    envelope(11, 200, "new-11", false),
                    envelope(12, 200, "new-12", false),
                ],
            )),
        ],
    );
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();

    run_one(&engine, &mut events, account, mailbox).await;
    assert_eq!(
        stored_subjects(&store, mailbox).await,
        vec!["old-1", "old-2"]
    );
    assert_eq!(
        store.sync_state().get(mailbox).await.unwrap().uid_validity,
        Some(100)
    );

    // A new UIDVALIDITY invalidates every cached UID: the old rows are gone.
    run_one(&engine, &mut events, account, mailbox).await;
    assert_eq!(
        stored_subjects(&store, mailbox).await,
        vec!["new-11", "new-12"]
    );
    let state = store.sync_state().get(mailbox).await.unwrap();
    assert_eq!(state.uid_validity, Some(200));
    assert_eq!(state.highest_uid, Some(12));
}

#[tokio::test]
async fn flag_updates_and_removals_are_applied() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    let seen = MessageFlags {
        seen: true,
        ..MessageFlags::default()
    };
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(
                UV,
                3,
                false,
                vec![
                    envelope(1, UV, "m1", false),
                    envelope(2, UV, "m2", false),
                    envelope(3, UV, "m3", false),
                ],
            )),
            Pass::Batch(changes(
                UV,
                3,
                vec![(
                    RemoteMessageId::Imap {
                        uid: 1,
                        uid_validity: UV,
                    },
                    seen,
                )],
                vec![RemoteMessageId::Imap {
                    uid: 3,
                    uid_validity: UV,
                }],
            )),
        ],
    );
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();

    run_one(&engine, &mut events, account, mailbox).await;
    assert_eq!(message_count(&store, mailbox).await, 3);

    run_one(&engine, &mut events, account, mailbox).await;
    // m3 removed, m1 marked seen, m2 untouched.
    assert_eq!(stored_subjects(&store, mailbox).await, vec!["m1", "m2"]);
    let items = store
        .messages()
        .list_page(mailbox, None, 10)
        .await
        .unwrap()
        .items;
    let m1 = items
        .iter()
        .find(|item| item.subject.as_deref() == Some("m1"))
        .unwrap();
    assert!(m1.flags.seen, "m1 should be marked seen");
    let m2 = items
        .iter()
        .find(|item| item.subject.as_deref() == Some("m2"))
        .unwrap();
    assert!(!m2.flags.seen, "m2 should be untouched");
}

#[tokio::test]
async fn temporary_error_retries_with_backoff_then_succeeds() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Fail(FailKind::Temporary),
            Pass::Batch(batch(
                UV,
                1,
                false,
                vec![envelope(1, UV, "recovered", false)],
            )),
        ],
    );
    // Real time with a tiny backoff keeps the test fast while still
    // exercising the backoff sleep between the failure and the retry.
    let config = SyncConfig {
        backoff_base: Duration::from_millis(2),
        backoff_cap: Duration::from_millis(20),
        ..test_config()
    };
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world.clone()), config);
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    // Exactly one retry: two sync attempts, and the job ultimately succeeds.
    assert_eq!(
        world.sync_calls(mailbox),
        2,
        "one transient failure then success"
    );
    assert_eq!(message_count(&store, mailbox).await, 1);
    let job_id = queued_job_id(&collected);
    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Succeeded);
    assert!(matches!(
        collected.last().unwrap(),
        SyncEvent::JobFinished { .. }
    ));
}

#[tokio::test]
async fn permanent_error_fails_without_retry() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(mailbox, vec![Pass::Fail(FailKind::Permanent)]);
    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    assert_eq!(
        world.sync_calls(mailbox),
        1,
        "permanent errors are not retried"
    );
    let job_id = queued_job_id(&collected);
    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Failed);
    match collected.last().unwrap() {
        SyncEvent::JobFailed { class, .. } => assert_eq!(*class, SyncErrorClass::Permanent),
        other => panic!("expected JobFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn authentication_error_fails_with_user_configuration_class() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.fail_connect(FailKind::Auth);
    world.set_plan(
        mailbox,
        vec![Pass::Batch(batch(
            UV,
            1,
            false,
            vec![envelope(1, UV, "unused", false)],
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
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    assert_eq!(
        world.sync_calls(mailbox),
        0,
        "auth fails before any sync pass"
    );
    let job_id = queued_job_id(&collected);
    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Failed);
    match collected.last().unwrap() {
        SyncEvent::JobFailed { class, .. } => {
            assert_eq!(*class, SyncErrorClass::UserConfiguration);
        }
        other => panic!("expected JobFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn cancellation_mid_job_marks_cancelled_and_stops() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    // Freeze the very first pass (incoming highest_uid is None).
    world.arm_pause(None);
    world.set_plan(
        mailbox,
        vec![Pass::Batch(batch(
            UV,
            1,
            false,
            vec![envelope(1, UV, "never", false)],
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
            mailbox_id: mailbox,
        })
        .await
        .unwrap();

    // Wait until the job is running (and thus parked at the gate), then cancel.
    let job_id = wait_for_started(&mut events).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    engine.cancel(job_id);

    // Cooperative cancellation surfaces as its own terminal event, never as a
    // failure, so the frontend does not render it with an error class.
    let terminal = wait_for_terminal_event(&mut events).await;
    match terminal {
        SyncEvent::JobCancelled { job_id: cancelled } => assert_eq!(cancelled, job_id),
        other => panic!("expected JobCancelled, got {other:?}"),
    }

    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Cancelled);
    // The recorded reason is the redacted cancellation message.
    assert_eq!(job.error.as_deref(), Some("operation was cancelled"));
    assert_eq!(world.sync_calls(mailbox), 0, "no pass completed");
    assert_eq!(message_count(&store, mailbox).await, 0);
}

#[tokio::test]
async fn crash_recovery_requeues_and_resumes_running_job() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(
                UV,
                2,
                true,
                vec![
                    envelope(1, UV, "b1-a", false),
                    envelope(2, UV, "b1-b", false),
                ],
            )),
            Pass::Batch(batch(
                UV,
                4,
                false,
                vec![
                    envelope(3, UV, "b2-a", false),
                    envelope(4, UV, "b2-b", false),
                ],
            )),
        ],
    );
    // Pause the second pass (incoming highest_uid == Some(2)).
    world.arm_pause(Some(2));

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();

    // Capture the job id and wait for the first batch to be persisted.
    let mut job_id = None;
    loop {
        match events.recv().await {
            Ok(SyncEvent::JobQueued { job_id: id, .. }) => job_id = Some(id),
            Ok(SyncEvent::BatchIngested { new_messages, .. }) => {
                assert_eq!(new_messages, 2);
                break;
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => panic!("channel closed before first batch"),
        }
    }
    let job_id = job_id.expect("JobQueued observed");

    // Simulate a crash: drop the engine while the worker is parked mid-job.
    drop(engine);

    // The job is left running and progress up to the first batch survives.
    let job = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Running);
    assert_eq!(
        store.sync_state().get(mailbox).await.unwrap().highest_uid,
        Some(2)
    );
    assert_eq!(message_count(&store, mailbox).await, 2);

    // A fresh engine requeues the stale running job and finishes it.
    world.disarm_pause();
    let engine2 = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events2 = engine2.subscribe();
    let collected = drain_until_terminal(&mut events2).await;

    assert!(matches!(
        collected.last().unwrap(),
        SyncEvent::JobFinished { .. }
    ));
    assert_eq!(message_count(&store, mailbox).await, 4);
    let resumed = store.sync_jobs().get(job_id).await.unwrap().unwrap();
    assert_eq!(resumed.state, JobState::Succeeded);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_account_jobs_never_run_concurrently() {
    let world = FakeWorld::new();
    let (store, account, inbox, archive) = store_with_two_mailboxes().await;
    world.set_plan(
        inbox,
        vec![Pass::Batch(batch(
            100,
            1,
            false,
            vec![envelope(1, 100, "inbox-1", false)],
        ))],
    );
    world.set_plan(
        archive,
        vec![Pass::Batch(batch(
            200,
            1,
            false,
            vec![envelope(1, 200, "archive-1", false)],
        ))],
    );
    // Widen the window in which a broken lock would let both passes overlap.
    world.set_pass_delay(Duration::from_millis(60));

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
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: archive,
        })
        .await
        .unwrap();

    // Wait for both jobs to finish.
    let mut finished = 0;
    while finished < 2 {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Ok(SyncEvent::JobFinished { .. })) => finished += 1,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => panic!("jobs did not both finish"),
        }
    }

    assert_eq!(
        world.max_active(account),
        1,
        "one account must never run two passes at once"
    );
    assert_eq!(message_count(&store, inbox).await, 1);
    assert_eq!(message_count(&store, archive).await, 1);
}

#[tokio::test]
async fn events_are_emitted_in_order_and_serialize() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    world.set_plan(
        mailbox,
        vec![
            Pass::Batch(batch(UV, 1, true, vec![envelope(1, UV, "e1", false)])),
            Pass::Batch(batch(UV, 2, false, vec![envelope(2, UV, "e2", false)])),
        ],
    );
    let engine = SyncEngine::start(store.clone(), FakeFactory::new(world), test_config());
    let mut events = engine.subscribe();
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(&mut events).await;

    let kinds: Vec<&'static str> = collected.iter().map(event_kind).collect();
    assert_eq!(
        kinds,
        vec!["queued", "started", "batch", "batch", "finished",],
        "events must arrive queued -> started -> batch* -> finished"
    );

    // Serde round-trip proves the events are wire-ready for the Tauri layer.
    let json = serde_json::to_string(&collected).unwrap();
    let back: Vec<SyncEvent> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, collected);
}

#[tokio::test]
async fn duplicate_enqueue_is_deduplicated() {
    let world = FakeWorld::new();
    let (store, account, mailbox) = store_with_mailbox().await;
    // Park the first job so it stays active while the duplicate is enqueued.
    world.arm_pause(None);
    world.set_plan(
        mailbox,
        vec![Pass::Batch(batch(
            UV,
            1,
            false,
            vec![envelope(1, UV, "x", false)],
        ))],
    );
    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory::new(world.clone()),
        test_config(),
    );
    let mut events = engine.subscribe();

    let request = SyncMailbox {
        account_id: account,
        mailbox_id: mailbox,
    };
    engine.enqueue(request).await.unwrap();
    engine.enqueue(request).await.unwrap();

    // Give the dispatcher time to process both commands; the first job parks
    // at the gate and stays active, so the duplicate must be skipped.
    tokio::time::sleep(Duration::from_millis(120)).await;

    // Exactly one job row exists, and it is the single running one.
    assert_eq!(
        store
            .sync_jobs()
            .list_by_state(JobState::Running)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .sync_jobs()
            .list_by_state(JobState::Pending)
            .await
            .unwrap()
            .len(),
        0
    );

    // Only one JobQueued was ever announced.
    let buffered = events_seen_so_far(&mut events);
    assert_eq!(
        count_kind(&buffered, "queued"),
        1,
        "the duplicate enqueue produced no second job"
    );
    assert_eq!(count_kind(&buffered, "started"), 1);

    engine.cancel_all();
}

// ---------------------------------------------------------------------------
// Small local helpers
// ---------------------------------------------------------------------------

/// Enqueues one job and drives it to a terminal event.
async fn run_one(
    engine: &SyncEngine<support::FakeProvider, FakeFactory>,
    events: &mut tokio::sync::broadcast::Receiver<SyncEvent>,
    account: axomail_core::AccountId,
    mailbox: axomail_core::MailboxId,
) {
    engine
        .enqueue(SyncMailbox {
            account_id: account,
            mailbox_id: mailbox,
        })
        .await
        .unwrap();
    let collected = drain_until_terminal(events).await;
    assert!(
        matches!(collected.last().unwrap(), SyncEvent::JobFinished { .. }),
        "expected the job to finish, got {:?}",
        collected.last()
    );
}

async fn wait_for_started(events: &mut tokio::sync::broadcast::Receiver<SyncEvent>) -> SyncJobId {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Ok(SyncEvent::JobStarted { job_id })) => return job_id,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => panic!("job never started"),
        }
    }
}

async fn wait_for_terminal_event(
    events: &mut tokio::sync::broadcast::Receiver<SyncEvent>,
) -> SyncEvent {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Ok(event))
                if matches!(
                    event,
                    SyncEvent::JobFinished { .. }
                        | SyncEvent::JobFailed { .. }
                        | SyncEvent::JobCancelled { .. }
                ) =>
            {
                return event;
            }
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => panic!("no terminal event"),
        }
    }
}

/// Non-blocking drain of whatever is currently buffered.
fn events_seen_so_far(events: &mut tokio::sync::broadcast::Receiver<SyncEvent>) -> Vec<SyncEvent> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => seen.push(event),
            Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => return seen,
        }
    }
}

fn count_kind(events: &[SyncEvent], kind: &str) -> usize {
    events
        .iter()
        .filter(|event| event_kind(event) == kind)
        .count()
}

fn event_kind(event: &SyncEvent) -> &'static str {
    match event {
        SyncEvent::JobQueued { .. } => "queued",
        SyncEvent::JobStarted { .. } => "started",
        SyncEvent::BatchIngested { .. } => "batch",
        SyncEvent::JobFinished { .. } => "finished",
        SyncEvent::JobCancelled { .. } => "cancelled",
        SyncEvent::JobFailed { .. } => "failed",
    }
}
