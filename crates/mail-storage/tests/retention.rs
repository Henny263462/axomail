//! Integration tests for the per-mailbox retention policy and the query that
//! selects the over-retention messages the maintenance pass acts on.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_core::{Address, EmailAddress, MailboxId, MessageFlags, RemoteMessageId, UtcTimestamp};
use axomail_storage::{NewMessage, RetentionAction, RetentionMode, RetentionPolicy, Store};

mod common;
use common::{sample_account, sample_mailbox};

/// A fixed "now" so the by-age arithmetic is deterministic.
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

/// Inserts a message with the given date, star, and cached-body state, and
/// returns its local id.
async fn insert(
    store: &Store,
    mailbox: MailboxId,
    uid: u32,
    date_unix: Option<i64>,
    flagged: bool,
    has_raw: bool,
) -> i64 {
    let from = Address::from_email(EmailAddress::parse("ada@example.org").unwrap());
    let to = Address::from_email(EmailAddress::parse("klaus@theferbers.eu").unwrap());
    let flags = MessageFlags {
        flagged,
        ..MessageFlags::default()
    };
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
        flags,
        size_bytes: Some(2048),
        preview: Some("preview".to_owned()),
        has_raw,
    };
    store.messages().insert(&message).await.unwrap().id.value()
}

#[tokio::test]
async fn default_policy_keeps_all() {
    let (store, mailbox) = store_with_mailbox().await;
    let policy = store.retention().get(mailbox).await.unwrap();
    assert_eq!(policy.mode, RetentionMode::KeepAll);
    assert_eq!(policy.action, RetentionAction::Trash);
    assert_eq!(policy.days, 30);
    assert_eq!(policy.max_count, 1000);
}

#[tokio::test]
async fn set_and_get_round_trip() {
    let (store, mailbox) = store_with_mailbox().await;
    let policy = RetentionPolicy {
        mode: RetentionMode::ByAge,
        days: 7,
        max_count: 50,
        action: RetentionAction::PurgeBody,
    };
    assert!(store.retention().set(mailbox, policy).await.unwrap());
    assert_eq!(store.retention().get(mailbox).await.unwrap(), policy);

    // An unknown mailbox reports that it does not exist.
    assert!(
        !store
            .retention()
            .set(MailboxId::new(9999), policy)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn keep_all_yields_no_candidates() {
    let (store, mailbox) = store_with_mailbox().await;
    insert(&store, mailbox, 1, Some(NOW - 1000 * DAY), false, true).await;
    let ids = store
        .retention()
        .over_retention(mailbox, NOW, false)
        .await
        .unwrap();
    assert!(ids.is_empty());
}

#[tokio::test]
async fn by_age_selects_old_unstarred_messages_oldest_first() {
    let (store, mailbox) = store_with_mailbox().await;
    let oldest = insert(&store, mailbox, 1, Some(NOW - 100 * DAY), false, true).await;
    let old = insert(&store, mailbox, 2, Some(NOW - 40 * DAY), false, true).await;
    // Within the window: kept.
    insert(&store, mailbox, 3, Some(NOW - 5 * DAY), false, true).await;
    // Old but starred: exempt.
    insert(&store, mailbox, 4, Some(NOW - 90 * DAY), true, true).await;
    // Old but undated: never selected by age.
    insert(&store, mailbox, 5, None, false, true).await;

    store
        .retention()
        .set(
            mailbox,
            RetentionPolicy {
                mode: RetentionMode::ByAge,
                days: 30,
                max_count: 1000,
                action: RetentionAction::Trash,
            },
        )
        .await
        .unwrap();

    let ids: Vec<i64> = store
        .retention()
        .over_retention(mailbox, NOW, false)
        .await
        .unwrap()
        .into_iter()
        .map(|id| id.value())
        .collect();
    assert_eq!(ids, vec![oldest, old]);
}

#[tokio::test]
async fn by_count_keeps_newest_n_and_exempts_stars() {
    let (store, mailbox) = store_with_mailbox().await;
    // Newest → oldest by date.
    let m_new = insert(&store, mailbox, 1, Some(NOW), false, true).await;
    let m_mid = insert(&store, mailbox, 2, Some(NOW - 10 * DAY), false, true).await;
    let m_old = insert(&store, mailbox, 3, Some(NOW - 20 * DAY), false, true).await;
    let m_oldest = insert(&store, mailbox, 4, Some(NOW - 30 * DAY), false, true).await;
    // A starred old message: occupies no exemption from the keep-count, but is
    // never a candidate.
    let _starred = insert(&store, mailbox, 5, Some(NOW - 40 * DAY), true, true).await;

    store
        .retention()
        .set(
            mailbox,
            RetentionPolicy {
                mode: RetentionMode::ByCount,
                days: 30,
                max_count: 2,
                action: RetentionAction::Trash,
            },
        )
        .await
        .unwrap();

    let ids: Vec<i64> = store
        .retention()
        .over_retention(mailbox, NOW, false)
        .await
        .unwrap()
        .into_iter()
        .map(|id| id.value())
        .collect();
    // Newest two (m_new, m_mid) kept; the rest, minus the starred one, selected
    // oldest first.
    assert_eq!(ids, vec![m_oldest, m_old]);
    assert!(!ids.contains(&m_new));
    assert!(!ids.contains(&m_mid));
}

#[tokio::test]
async fn cached_only_restricts_to_messages_with_a_body() {
    let (store, mailbox) = store_with_mailbox().await;
    let cached = insert(&store, mailbox, 1, Some(NOW - 100 * DAY), false, true).await;
    // Old and over-retention but no cached body: excluded when cached_only.
    insert(&store, mailbox, 2, Some(NOW - 100 * DAY), false, false).await;

    store
        .retention()
        .set(
            mailbox,
            RetentionPolicy {
                mode: RetentionMode::ByAge,
                days: 30,
                max_count: 1000,
                action: RetentionAction::PurgeBody,
            },
        )
        .await
        .unwrap();

    let with_body: Vec<i64> = store
        .retention()
        .over_retention(mailbox, NOW, true)
        .await
        .unwrap()
        .into_iter()
        .map(|id| id.value())
        .collect();
    assert_eq!(with_body, vec![cached]);

    // Without the cached-only restriction both old messages qualify.
    let all = store
        .retention()
        .over_retention(mailbox, NOW, false)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
}
