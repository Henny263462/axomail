//! Integration tests for the calendar-invitation (iMIP RSVP) repository:
//! upsert-by-message, response update in place, CANCEL-by-UID, and FK cascade
//! when the carrying message is deleted.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, MailboxId, MessageId};
use axomail_storage::{NewInvite, RsvpStatus, Store};
use common::{sample_account, sample_mailbox, sample_message};

/// Opens a store with one account and one mailbox.
async fn setup() -> (Store, AccountId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account: AccountId = store
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
    (store, account, mailbox)
}

async fn insert_message(store: &Store, mailbox: MailboxId, subject: &str) -> MessageId {
    store
        .messages()
        .insert(&sample_message(mailbox, subject, Some(1_700_000_000)))
        .await
        .unwrap()
        .id
}

fn new_invite(message: MessageId, status: RsvpStatus) -> NewInvite {
    NewInvite {
        message_id: message,
        uid: "event-1@example.org".to_owned(),
        summary: Some("Quarterly plan".to_owned()),
        organizer: Some("ada@example.org".to_owned()),
        starts_at: Some(1_784_545_200),
        ends_at: Some(1_784_548_800),
        location: Some("Room 5".to_owned()),
        my_status: status,
        sequence: 2,
    }
}

#[tokio::test]
async fn upsert_records_then_updates_response_in_place() {
    let (store, _account, mailbox) = setup().await;
    let message = insert_message(&store, mailbox, "Invitation").await;

    let stored = store
        .calendar_invites()
        .upsert(&new_invite(message, RsvpStatus::None))
        .await
        .unwrap();
    assert_eq!(stored.uid, "event-1@example.org");
    assert_eq!(stored.my_status, RsvpStatus::None);
    assert!(!stored.cancelled);

    // A second upsert for the same message updates the response, not inserts.
    let updated = store
        .calendar_invites()
        .upsert(&new_invite(message, RsvpStatus::Accepted))
        .await
        .unwrap();
    assert_eq!(updated.id, stored.id, "same row, updated in place");
    assert_eq!(updated.my_status, RsvpStatus::Accepted);

    let fetched = store
        .calendar_invites()
        .get_by_message(message)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.my_status, RsvpStatus::Accepted);
    assert_eq!(fetched.summary.as_deref(), Some("Quarterly plan"));
    assert_eq!(fetched.starts_at, Some(1_784_545_200));
}

#[tokio::test]
async fn get_by_message_is_none_without_a_row() {
    let (store, _account, mailbox) = setup().await;
    let message = insert_message(&store, mailbox, "Plain message").await;
    assert!(
        store
            .calendar_invites()
            .get_by_message(message)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn cancel_updates_an_unanswered_request_scoped_to_organizer_and_uid() {
    let (store, account, mailbox) = setup().await;
    let request = insert_message(&store, mailbox, "Invitation").await;
    let other = insert_message(&store, mailbox, "Unrelated invitation").await;

    // An un-answered (pending) REQUEST — the case a CANCEL must still reach.
    store
        .calendar_invites()
        .upsert(&new_invite(request, RsvpStatus::None))
        .await
        .unwrap();
    // A second invite carrying a different UID must be left untouched.
    let mut different = new_invite(other, RsvpStatus::Tentative);
    different.uid = "event-2@example.org".to_owned();
    store.calendar_invites().upsert(&different).await.unwrap();

    let count = store
        .calendar_invites()
        .mark_cancelled(account, "event-1@example.org", Some("ada@example.org"), 2)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let cancelled = store
        .calendar_invites()
        .get_by_message(request)
        .await
        .unwrap()
        .unwrap();
    assert!(
        cancelled.cancelled,
        "an un-answered REQUEST is cancelled by a matching CANCEL"
    );
    assert_eq!(cancelled.my_status, RsvpStatus::None);

    let untouched = store
        .calendar_invites()
        .get_by_message(other)
        .await
        .unwrap()
        .unwrap();
    assert!(!untouched.cancelled, "a different UID is not cancelled");
}

#[tokio::test]
async fn cancel_with_a_mismatched_organizer_does_not_cancel() {
    let (store, account, mailbox) = setup().await;
    let request = insert_message(&store, mailbox, "Invitation").await;
    store
        .calendar_invites()
        .upsert(&new_invite(request, RsvpStatus::None))
        .await
        .unwrap();

    // A forged CANCEL with the right UID but a different organizer is rejected.
    let count = store
        .calendar_invites()
        .mark_cancelled(
            account,
            "event-1@example.org",
            Some("attacker@evil.example"),
            9,
        )
        .await
        .unwrap();
    assert_eq!(count, 0, "a mismatched organizer must not cancel");
    // An organizer-less CANCEL authenticates nothing.
    assert_eq!(
        store
            .calendar_invites()
            .mark_cancelled(account, "event-1@example.org", None, 9)
            .await
            .unwrap(),
        0
    );
    assert!(
        !store
            .calendar_invites()
            .get_by_message(request)
            .await
            .unwrap()
            .unwrap()
            .cancelled
    );
}

#[tokio::test]
async fn cancel_from_another_account_does_not_cancel() {
    let (store, _account, mailbox) = setup().await;
    let request = insert_message(&store, mailbox, "Invitation").await;
    store
        .calendar_invites()
        .upsert(&new_invite(request, RsvpStatus::None))
        .await
        .unwrap();

    // A second account whose UID happens to collide must not reach this invite.
    let intruder = store
        .accounts()
        .insert(&sample_account("Other", "eve@example.org"))
        .await
        .unwrap()
        .id;
    let count = store
        .calendar_invites()
        .mark_cancelled(intruder, "event-1@example.org", Some("ada@example.org"), 9)
        .await
        .unwrap();
    assert_eq!(count, 0, "a CANCEL scoped to another account cannot cancel");
}

#[tokio::test]
async fn a_stale_cancel_sequence_does_not_apply() {
    let (store, account, mailbox) = setup().await;
    let request = insert_message(&store, mailbox, "Invitation").await;
    // Stored REQUEST is at SEQUENCE 2 (see `new_invite`).
    store
        .calendar_invites()
        .upsert(&new_invite(request, RsvpStatus::None))
        .await
        .unwrap();

    // A CANCEL for an older revision (SEQUENCE 1) is stale and does not apply.
    assert_eq!(
        store
            .calendar_invites()
            .mark_cancelled(account, "event-1@example.org", Some("ada@example.org"), 1)
            .await
            .unwrap(),
        0
    );
    // A CANCEL at the current revision applies.
    assert_eq!(
        store
            .calendar_invites()
            .mark_cancelled(account, "event-1@example.org", Some("ada@example.org"), 2)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn is_event_cancelled_reflects_a_scoped_marker() {
    let (store, account, mailbox) = setup().await;
    let request = insert_message(&store, mailbox, "Invitation").await;
    store
        .calendar_invites()
        .upsert(&new_invite(request, RsvpStatus::None))
        .await
        .unwrap();
    store
        .calendar_invites()
        .mark_cancelled(account, "event-1@example.org", Some("ada@example.org"), 2)
        .await
        .unwrap();

    assert!(
        store
            .calendar_invites()
            .is_event_cancelled(account, "event-1@example.org", Some("ada@example.org"), 2)
            .await
            .unwrap()
    );
    // A mismatched organizer sees no cancellation.
    assert!(
        !store
            .calendar_invites()
            .is_event_cancelled(account, "event-1@example.org", Some("eve@evil.example"), 2)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn upsert_does_not_resurrect_a_cancelled_event() {
    let (store, account, mailbox) = setup().await;
    let message = insert_message(&store, mailbox, "Invitation").await;
    store
        .calendar_invites()
        .upsert(&new_invite(message, RsvpStatus::None))
        .await
        .unwrap();
    store
        .calendar_invites()
        .mark_cancelled(account, "event-1@example.org", Some("ada@example.org"), 2)
        .await
        .unwrap();

    // Responding after a cancel keeps the cancelled flag set.
    let after = store
        .calendar_invites()
        .upsert(&new_invite(message, RsvpStatus::Declined))
        .await
        .unwrap();
    assert!(after.cancelled);
    assert_eq!(after.my_status, RsvpStatus::Declined);
}

#[tokio::test]
async fn deleting_the_account_cascades_to_the_invite() {
    let (store, account, mailbox) = setup().await;
    let message = insert_message(&store, mailbox, "Invitation").await;
    store
        .calendar_invites()
        .upsert(&new_invite(message, RsvpStatus::Accepted))
        .await
        .unwrap();

    // Removing the account cascades messages away, and the invitation with them
    // (the FK on `calendar_invites.message_id` is ON DELETE CASCADE).
    assert!(store.accounts().delete(account).await.unwrap());
    assert!(
        store
            .calendar_invites()
            .get_by_message(message)
            .await
            .unwrap()
            .is_none()
    );
}
