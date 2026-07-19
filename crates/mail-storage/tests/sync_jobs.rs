//! Integration test for the account-scoped `sync_jobs` dedup key: an
//! account-scoped job kind (`mailbox_id NULL`, e.g. `Discover`) must not
//! collapse across accounts. Regression test for the documented
//! `(kind, mailbox_id)`-only dedup that keyed every account's NULL-mailbox
//! job into one row.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::AccountId;
use axomail_storage::{JobKind, NewSyncJob, Store, SyncJob};
use common::sample_account;

/// Enqueues an account-scoped `Discover` job (mailbox_id NULL) for `account`.
async fn enqueue_discover(store: &Store, account: AccountId) -> Option<SyncJob> {
    store
        .sync_jobs()
        .create_if_absent(&NewSyncJob {
            kind: JobKind::Discover,
            account_id: Some(account),
            mailbox_id: None,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn account_scoped_dedup_does_not_collapse_across_accounts() {
    let store = Store::open_in_memory().await.unwrap();
    let account_a = store
        .accounts()
        .insert(&sample_account("A", "a@example.org"))
        .await
        .unwrap()
        .id;
    let account_b = store
        .accounts()
        .insert(&sample_account("B", "b@example.org"))
        .await
        .unwrap()
        .id;

    // Each account's first Discover is created as its own row.
    let job_a = enqueue_discover(&store, account_a)
        .await
        .expect("account A discover created");
    let job_b = enqueue_discover(&store, account_b)
        .await
        .expect("account B discover created, not collapsed into A's");
    assert_ne!(job_a.id, job_b.id);
    assert_eq!(job_a.account_id, Some(account_a));
    assert_eq!(job_b.account_id, Some(account_b));

    // Re-enqueuing the same account's Discover still dedups (same account,
    // same NULL mailbox, already pending).
    assert!(
        enqueue_discover(&store, account_a).await.is_none(),
        "a duplicate Discover for account A is skipped"
    );
    assert!(
        enqueue_discover(&store, account_b).await.is_none(),
        "a duplicate Discover for account B is skipped"
    );
}
