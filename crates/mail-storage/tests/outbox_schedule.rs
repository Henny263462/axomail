//! Integration tests for the outbox "send later" scheduling guard: a row
//! whose `next_attempt_unix` is in the future is withheld from the worker
//! until it is due, and clearing the schedule makes it immediately eligible.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::AccountId;
use axomail_storage::{NewOutbox, Store};
use common::sample_account;

/// Current Unix seconds.
fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Builds an outbox row for `account` scheduled at `next_attempt_unix`.
fn scheduled(account: AccountId, subject: &str, next_attempt_unix: Option<i64>) -> NewOutbox {
    NewOutbox {
        account,
        draft_id: None,
        identity_id: None,
        subject: subject.to_owned(),
        body_text: "body".to_owned(),
        to: vec!["bob@example.org".to_owned()],
        cc: Vec::new(),
        bcc: Vec::new(),
        next_attempt_unix,
    }
}

async fn account(store: &Store) -> AccountId {
    store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn list_queued_withholds_future_rows_and_yields_due_ones() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store).await;

    let due_now = store
        .outbox()
        .create(&scheduled(acc, "Immediate", None))
        .await
        .unwrap();
    let due_past = store
        .outbox()
        .create(&scheduled(acc, "Past due", Some(now() - 60)))
        .await
        .unwrap();
    let future = store
        .outbox()
        .create(&scheduled(acc, "Later", Some(now() + 3_600)))
        .await
        .unwrap();

    let queued = store.outbox().list_queued().await.unwrap();
    let ids: Vec<_> = queued.iter().map(|row| row.id).collect();
    assert!(ids.contains(&due_now.id), "unscheduled rows are due");
    assert!(ids.contains(&due_past.id), "a past schedule is due");
    assert!(
        !ids.contains(&future.id),
        "a future schedule must be withheld"
    );

    // The future row still carries its schedule for the UI to display.
    let stored = store.outbox().get(future.id).await.unwrap().unwrap();
    assert!(stored.next_attempt_unix.unwrap() > now());
}

#[tokio::test]
async fn delete_if_queued_refuses_a_row_that_is_no_longer_queued() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store).await;
    let row = store
        .outbox()
        .create(&scheduled(acc, "Later", Some(now() + 3_600)))
        .await
        .unwrap();

    // A concurrent send claims the row (queued -> sending).
    assert!(store.outbox().begin_send(row.id).await.unwrap());

    // The guarded delete must not remove a row that has left `queued`: this is
    // the atomicity a plain unconditional delete could not provide.
    assert!(
        !store.outbox().delete_if_queued(row.id).await.unwrap(),
        "a sending row must not be deletable via the queued guard"
    );
    assert!(
        store.outbox().get(row.id).await.unwrap().is_some(),
        "the in-flight row must survive the guarded delete"
    );

    // A still-queued row is removed and reported as removed.
    let queued = store
        .outbox()
        .create(&scheduled(acc, "Cancel me", None))
        .await
        .unwrap();
    assert!(store.outbox().delete_if_queued(queued.id).await.unwrap());
    assert!(store.outbox().get(queued.id).await.unwrap().is_none());
}

#[tokio::test]
async fn clearing_the_schedule_makes_a_future_row_due() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store).await;
    let future = store
        .outbox()
        .create(&scheduled(acc, "Later", Some(now() + 3_600)))
        .await
        .unwrap();

    assert!(
        !store
            .outbox()
            .list_queued()
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == future.id)
    );

    assert!(store.outbox().clear_schedule(future.id).await.unwrap());

    let queued = store.outbox().list_queued().await.unwrap();
    assert!(
        queued.iter().any(|r| r.id == future.id),
        "the row is due once its schedule is cleared"
    );
    let stored = store.outbox().get(future.id).await.unwrap().unwrap();
    assert!(stored.next_attempt_unix.is_none());
}
