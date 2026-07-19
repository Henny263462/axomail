//! Integration test for `sync_account`: it enqueues one job per syncable
//! mailbox (inbox/archive/custom) and returns the created job ids. A stub
//! provider factory avoids any real network dialog.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;

use axomail_core::{
    Account, AppendOutcome, FlagDelta, FlagSet, IncomingMailProvider, MailError, MailboxName,
    MailboxRole, RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::sync::sync_account_impl;
use axomail_storage::Store;
use axomail_sync::{ProviderFactory, SyncConfig, SyncEngine, SyncJobId};
use common::{secret_stores, seed_standard_folders, valid_input};

/// A provider that never connects; the sync worker fails fast, but the
/// `JobQueued` events (emitted before any worker runs) are what the command
/// collects.
#[derive(Default)]
struct StubProvider;

impl IncomingMailProvider for StubProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Err(MailError::Configuration {
            message: "stub provider never connects".to_owned(),
        })
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Err(MailError::Cancelled)
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        Err(MailError::Cancelled)
    }
    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn store_flags(
        &mut self,
        _id: &RemoteMessageId,
        _delta: &FlagDelta,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn move_message(
        &mut self,
        _id: &RemoteMessageId,
        _target: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn append_message(
        &mut self,
        _mailbox: &MailboxName,
        _flags: &FlagSet,
        _raw: &[u8],
    ) -> Result<AppendOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn create_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn rename_mailbox(
        &mut self,
        _from: &MailboxName,
        _to: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn set_mailbox_subscribed(
        &mut self,
        _path: &MailboxName,
        _subscribed: bool,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

struct StubFactory;

impl ProviderFactory<StubProvider> for StubFactory {
    async fn make(&self, _account: &Account) -> Result<StubProvider, MailError> {
        Ok(StubProvider)
    }
}

#[tokio::test]
async fn sync_account_enqueues_only_syncable_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let created = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    // IMAP seeds only the inbox; add the standard folders so the tree spans
    // both syncable (inbox/archive) and skipped (drafts/sent/trash) roles.
    seed_standard_folders(&store, axomail_core::AccountId::new(created.id)).await;
    let summary = axomail_desktop_lib::commands::accounts::list_accounts_impl(&store)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    // The tree is Inbox, Drafts, Sent, Archive, Trash — so only two
    // mailboxes (Inbox, Archive) are syncable.
    let syncable: HashSet<i64> = summary
        .mailboxes
        .iter()
        .filter(|mailbox| {
            matches!(
                mailbox.role,
                MailboxRole::Inbox | MailboxRole::Archive | MailboxRole::Custom
            )
        })
        .map(|mailbox| mailbox.id)
        .collect();
    assert_eq!(syncable.len(), 2, "default account has inbox + archive");

    let engine = SyncEngine::start(store.clone(), StubFactory, SyncConfig::default());
    let job_ids = sync_account_impl(&store, &engine, summary.id)
        .await
        .unwrap();

    assert_eq!(job_ids.len(), 2, "one job per syncable mailbox");

    // Every returned job targets a syncable mailbox, never a skipped one.
    for id in job_ids {
        let job = store
            .sync_jobs()
            .get(SyncJobId::new(id))
            .await
            .unwrap()
            .expect("job row exists");
        let mailbox = job.mailbox_id.expect("mailbox-scoped job").value();
        assert!(
            syncable.contains(&mailbox),
            "job {id} targets non-syncable mailbox {mailbox}"
        );
    }
}

#[tokio::test]
async fn sync_account_rejects_unknown_account() {
    let store = Store::open_in_memory().await.unwrap();
    let engine = SyncEngine::start(store.clone(), StubFactory, SyncConfig::default());

    let error = sync_account_impl(&store, &engine, 4242)
        .await
        .expect_err("unknown account must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}
