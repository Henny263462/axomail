//! Shared fake incoming providers and engines for command integration tests.
//!
//! [`StubProvider`] never connects: it is enough for tests that only need the
//! command to enqueue a pending op and trigger an `OpReplay` (the replay job
//! then fails fast without touching the freshly enqueued ops). [`CannedProvider`]
//! serves a fixed raw body from `fetch_message`, for the reading pane's
//! lazy-fetch path (offline body download).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use axomail_core::{
    Account, AppendOutcome, FlagDelta, FlagSet, IncomingMailProvider, MailError, MailboxName,
    RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_storage::Store;
use axomail_sync::{ProviderFactory, SyncConfig, SyncEngine};

/// An empty, terminal sync outcome (no new messages, nothing removed).
fn empty_outcome() -> SyncOutcome {
    SyncOutcome {
        uid_validity: None,
        highest_uid: None,
        highest_modseq: None,
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    }
}

/// A provider that never connects. The write-back / tag commands only enqueue
/// and trigger a replay; the spawned replay job simply fails fast here.
#[derive(Default)]
pub struct StubProvider;

impl IncomingMailProvider for StubProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Err(MailError::Configuration {
            message: "stub never connects".to_owned(),
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

/// Builds providers that never connect.
pub struct StubFactory;

impl ProviderFactory<StubProvider> for StubFactory {
    async fn make(&self, _account: &Account) -> Result<StubProvider, MailError> {
        Ok(StubProvider)
    }
}

/// A running engine backed by [`StubProvider`], for op-replay-trigger tests.
pub fn stub_engine(store: &Store) -> SyncEngine<StubProvider, StubFactory> {
    SyncEngine::start(store.clone(), StubFactory, SyncConfig::default())
}

/// A provider that connects, selects any mailbox with an empty sync pass, and
/// serves a fixed raw body from `fetch_message` — the reading pane's on-demand
/// body fetch. Per-message write ops are unused and error.
pub struct CannedProvider {
    bytes: Vec<u8>,
}

impl IncomingMailProvider for CannedProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Err(MailError::Cancelled)
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Ok(empty_outcome())
    }
    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        Ok(RawMessage {
            remote_id: id.clone(),
            bytes: self.bytes.clone(),
        })
    }
    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Ok(())
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

/// Builds [`CannedProvider`]s that all serve the same raw body.
pub struct CannedFactory {
    pub bytes: Vec<u8>,
}

impl ProviderFactory<CannedProvider> for CannedFactory {
    async fn make(&self, _account: &Account) -> Result<CannedProvider, MailError> {
        Ok(CannedProvider {
            bytes: self.bytes.clone(),
        })
    }
}
