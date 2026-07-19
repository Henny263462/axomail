//! Integration tests for the per-mailbox offline policy and the queries that
//! drive sync-time body download.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_core::{
    Address, EmailAddress, MailboxId, MessageFlags, MessageId, RemoteMessageId, UtcTimestamp,
};
use axomail_storage::{NewMessage, OfflineMode, Store};

mod common;
use common::{sample_account, sample_mailbox};

/// A fixed "now" so the recent-window arithmetic is deterministic.
const NOW: i64 = 1_700_000_000;
const DAY: i64 = 86_400;

async fn store_with_mailbox() -> (Store, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    (store, mailbox.id)
}

/// Inserts a message with an IMAP remote identity and an explicit date and
/// cached-body flag, returning its local id.
async fn insert_remote(
    store: &Store,
    mailbox: MailboxId,
    uid: u32,
    date_unix: Option<i64>,
    has_raw: bool,
) -> i64 {
    let from = Address::from_email(EmailAddress::parse("ada@example.org").unwrap());
    let to = Address::from_email(EmailAddress::parse("klaus@theferbers.eu").unwrap());
    let message = NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: 1,
        }),
        subject: Some(format!("Message {uid}")),
        from: vec![from],
        to: vec![to],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: date_unix.map(UtcTimestamp::from_unix_seconds),
        flags: MessageFlags::default(),
        size_bytes: Some(2048),
        preview: Some("preview".to_owned()),
        has_raw,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

#[tokio::test]
async fn default_policy_is_recent_30_days() {
    let (store, mailbox) = store_with_mailbox().await;
    let policy = store.offline().get(mailbox).await.unwrap();
    assert_eq!(policy.mode, OfflineMode::Recent);
    assert_eq!(policy.recent_days, 30);
}

#[tokio::test]
async fn set_and_get_round_trip() {
    let (store, mailbox) = store_with_mailbox().await;
    assert!(
        store
            .offline()
            .set(mailbox, OfflineMode::All, 7)
            .await
            .unwrap()
    );
    let policy = store.offline().get(mailbox).await.unwrap();
    assert_eq!(policy.mode, OfflineMode::All);
    // recent_days is stored even when the mode ignores it.
    assert_eq!(policy.recent_days, 7);

    // Setting a policy for an unknown mailbox reports that it does not exist.
    assert!(
        !store
            .offline()
            .set(MailboxId::new(9999), OfflineMode::None, 5)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn none_mode_yields_no_download_candidates() {
    let (store, mailbox) = store_with_mailbox().await;
    insert_remote(&store, mailbox, 1, Some(NOW), false).await;
    store
        .offline()
        .set(mailbox, OfflineMode::None, 30)
        .await
        .unwrap();

    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    assert!(plan.candidates.is_empty());
    assert!(!plan.force_full);
}

#[tokio::test]
async fn recent_mode_covers_only_recent_undownloaded_remote_messages() {
    let (store, mailbox) = store_with_mailbox().await;
    let recent = insert_remote(&store, mailbox, 1, Some(NOW - DAY), false).await;
    // Old (outside the 30-day window): excluded.
    insert_remote(&store, mailbox, 2, Some(NOW - 100 * DAY), false).await;
    // Recent but already cached: excluded.
    insert_remote(&store, mailbox, 3, Some(NOW - DAY), true).await;
    // Recent but local-only (no remote identity): excluded.
    let local = NewMessage {
        mailbox,
        remote_id: None,
        subject: Some("local".to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("ada@example.org").unwrap(),
        )],
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(NOW)),
        flags: MessageFlags::default(),
        size_bytes: None,
        preview: None,
        has_raw: false,
    };
    store.messages().insert(&local).await.unwrap();

    store
        .offline()
        .set(mailbox, OfflineMode::Recent, 30)
        .await
        .unwrap();
    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    let ids: Vec<i64> = plan
        .candidates
        .iter()
        .map(|c| c.message_id.value())
        .collect();
    assert_eq!(ids, vec![recent]);
}

#[tokio::test]
async fn all_mode_covers_every_undownloaded_remote_message_newest_first() {
    let (store, mailbox) = store_with_mailbox().await;
    let old = insert_remote(&store, mailbox, 1, Some(NOW - 100 * DAY), false).await;
    let new = insert_remote(&store, mailbox, 2, Some(NOW), false).await;
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();

    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    let ids: Vec<i64> = plan
        .candidates
        .iter()
        .map(|c| c.message_id.value())
        .collect();
    // Newest first.
    assert_eq!(ids, vec![new, old]);
}

#[tokio::test]
async fn pending_full_download_overrides_a_none_policy() {
    let (store, mailbox) = store_with_mailbox().await;
    insert_remote(&store, mailbox, 1, Some(NOW - 100 * DAY), false).await;
    store
        .offline()
        .set(mailbox, OfflineMode::None, 30)
        .await
        .unwrap();
    assert!(
        store
            .offline()
            .set_pending_full_download(mailbox, true)
            .await
            .unwrap()
    );

    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    assert!(plan.force_full);
    assert_eq!(plan.candidates.len(), 1);
}

#[tokio::test]
async fn download_plan_is_bounded_by_limit() {
    let (store, mailbox) = store_with_mailbox().await;
    for uid in 1..=5 {
        insert_remote(&store, mailbox, uid, Some(NOW), false).await;
    }
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();

    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 2)
        .await
        .unwrap();
    assert_eq!(plan.candidates.len(), 2);
}

#[tokio::test]
async fn mark_downloaded_sets_has_raw_and_indexes_body_for_search() {
    let (store, mailbox) = store_with_mailbox().await;
    let id = insert_remote(&store, mailbox, 1, Some(NOW), false).await;
    let message_id = MessageId::new(id);

    // Before download: no body indexed, so the unique term does not match.
    let before = store.search().search(&[], "zqxwomble", 10).await.unwrap();
    assert!(before.is_empty());

    // Before download the message is a candidate under 'all'.
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();
    let before_plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    assert_eq!(before_plan.candidates.len(), 1);

    store
        .offline()
        .mark_downloaded(message_id, "the body mentions zqxwomble once")
        .await
        .unwrap();

    // has_raw is now set, so the message drops out of the download plan.
    let after_plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    assert!(after_plan.candidates.is_empty());

    // And its full body text is searchable without ever opening it.
    let after = store.search().search(&[], "zqxwomble", 10).await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].message_id, message_id);
}

#[tokio::test]
async fn cached_outside_window_returns_stale_recent_and_all_none() {
    let (store, mailbox) = store_with_mailbox().await;
    let stale = insert_remote(&store, mailbox, 1, Some(NOW - 100 * DAY), true).await;
    let fresh = insert_remote(&store, mailbox, 2, Some(NOW), true).await;

    store
        .offline()
        .set(mailbox, OfflineMode::Recent, 30)
        .await
        .unwrap();
    let outside: Vec<i64> = store
        .offline()
        .cached_outside_window(mailbox, NOW)
        .await
        .unwrap()
        .into_iter()
        .map(|id| id.value())
        .collect();
    assert_eq!(outside, vec![stale]);

    // Under 'all' nothing is outside the window.
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();
    assert!(
        store
            .offline()
            .cached_outside_window(mailbox, NOW)
            .await
            .unwrap()
            .is_empty()
    );

    // Under 'none' every cached body is purgeable.
    store
        .offline()
        .set(mailbox, OfflineMode::None, 30)
        .await
        .unwrap();
    let mut outside_none: Vec<i64> = store
        .offline()
        .cached_outside_window(mailbox, NOW)
        .await
        .unwrap()
        .into_iter()
        .map(|id| id.value())
        .collect();
    outside_none.sort_unstable();
    assert_eq!(outside_none, vec![stale, fresh]);
}

#[tokio::test]
async fn clear_has_raw_flips_the_flag_back() {
    let (store, mailbox) = store_with_mailbox().await;
    let id = insert_remote(&store, mailbox, 1, Some(NOW), true).await;
    let message_id = MessageId::new(id);
    store
        .offline()
        .set(mailbox, OfflineMode::All, 30)
        .await
        .unwrap();

    // has_raw = 1, so it is not a download candidate yet.
    assert!(
        store
            .offline()
            .download_plan(mailbox, NOW, 50)
            .await
            .unwrap()
            .candidates
            .is_empty()
    );

    assert!(store.offline().clear_has_raw(message_id).await.unwrap());

    // Cleared: the message becomes a download candidate again.
    let plan = store
        .offline()
        .download_plan(mailbox, NOW, 50)
        .await
        .unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].message_id, message_id);
}
