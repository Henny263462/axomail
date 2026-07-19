//! Test support: a scripted, instrumented [`IncomingMailProvider`] and
//! its factory, store fixtures, and event helpers.
//!
//! This is *test* infrastructure — deliberately controllable and
//! observable — not a product fake. Each mailbox is driven by a queue of
//! scripted passes; a shared gate lets a test freeze a job mid-pass to
//! exercise cancellation and crash recovery, and per-account counters let
//! a test prove that one account never runs two passes at once.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU16;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{
    Account, AccountAuth, AccountId, Address, ConnectionSecurity, CredentialRef, EmailAddress,
    FlagDelta, IncomingMailProvider, IncomingProtocol, IncomingServerConfig, MailError, MailboxId,
    MailboxName, MailboxRole, MessageFlags, OutgoingServerConfig, RawMessage, RemoteEnvelope,
    RemoteMailboxState, RemoteMessageId, ServerEndpoint, SyncOutcome, SyncRequest, UtcTimestamp,
};
use axomail_storage::{NewAccount, NewMailbox, Store};
use axomail_sync::{ProviderFactory, SyncConfig, SyncEvent, SyncJobId};
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;

/// How a scripted `sync` pass should respond.
#[derive(Debug, Clone)]
pub enum Pass {
    /// Return this outcome.
    Batch(SyncOutcome),
    /// Fail with an error of this class.
    Fail(FailKind),
}

/// The error a scripted step should raise.
#[derive(Debug, Clone, Copy)]
pub enum FailKind {
    /// Maps to a temporary (retriable) error.
    Temporary,
    /// Maps to a permanent error.
    Permanent,
    /// Maps to an authentication error (user-configuration class).
    Auth,
}

impl FailKind {
    fn build(self) -> MailError {
        match self {
            Self::Temporary => MailError::Network {
                message: "scripted transient failure".to_owned(),
            },
            Self::Permanent => MailError::Protocol {
                message: "scripted permanent failure".to_owned(),
            },
            Self::Auth => MailError::Authentication,
        }
    }
}

/// Shared, observable world backing every provider instance.
#[derive(Default)]
pub struct FakeWorld {
    plans: Mutex<HashMap<MailboxId, VecDeque<Pass>>>,
    connect_errors: Mutex<VecDeque<FailKind>>,
    raw_messages: Mutex<HashMap<RemoteMessageId, Vec<u8>>>,
    fetch_calls: Mutex<usize>,
    sync_calls: Mutex<HashMap<MailboxId, usize>>,
    connect_calls: Mutex<usize>,
    discovery: Mutex<HashMap<AccountId, RemoteMailboxState>>,
    active: Mutex<HashMap<AccountId, usize>>,
    max_active: Mutex<HashMap<AccountId, usize>>,
    pause_at: Mutex<Option<Option<u32>>>,
    pass_delay: Mutex<Option<Duration>>,
    gate: Notify,
    // Write-back observability (track 2.1). Every per-message op the write-back
    // executor issues is recorded here in call order, and a message's IMAP UID
    // can be "poisoned" so its op fails with a scripted error class.
    select_calls: Mutex<Vec<MailboxName>>,
    flag_calls: Mutex<Vec<(RemoteMessageId, FlagDelta)>>,
    move_calls: Mutex<Vec<(RemoteMessageId, MailboxName)>>,
    poison: Mutex<HashMap<u32, FailKind>>,
}

impl FakeWorld {
    /// Creates an empty world.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Installs the scripted passes for a mailbox.
    pub fn set_plan(&self, mailbox: MailboxId, passes: Vec<Pass>) {
        lock(&self.plans).insert(mailbox, passes.into());
    }

    /// Queues an error to be returned by the next `connect` call.
    pub fn fail_connect(&self, kind: FailKind) {
        lock(&self.connect_errors).push_back(kind);
    }

    /// Scripts the raw RFC 5322 bytes a `fetch_message` call returns for a
    /// remote id (used by the offline-download tests).
    pub fn set_raw(&self, id: RemoteMessageId, bytes: impl Into<Vec<u8>>) {
        lock(&self.raw_messages).insert(id, bytes.into());
    }

    /// Number of `fetch_message` calls observed overall.
    #[must_use]
    pub fn fetch_calls(&self) -> usize {
        *lock(&self.fetch_calls)
    }

    /// Installs the folder structure `discover` reports for an account.
    pub fn set_discovery(&self, account: AccountId, state: RemoteMailboxState) {
        lock(&self.discovery).insert(account, state);
    }

    /// Arms the mid-pass gate: a pass whose incoming
    /// `state.highest_uid == target` blocks until the job is cancelled or
    /// aborted.
    pub fn arm_pause(&self, target: Option<u32>) {
        *lock(&self.pause_at) = Some(target);
    }

    /// Disarms the mid-pass gate so no future pass blocks.
    pub fn disarm_pause(&self) {
        *lock(&self.pause_at) = None;
    }

    /// Adds an artificial delay inside every pass, widening the window in
    /// which a serialization bug would surface.
    pub fn set_pass_delay(&self, delay: Duration) {
        *lock(&self.pass_delay) = Some(delay);
    }

    /// Number of `sync` calls observed for a mailbox.
    #[must_use]
    pub fn sync_calls(&self, mailbox: MailboxId) -> usize {
        lock(&self.sync_calls).get(&mailbox).copied().unwrap_or(0)
    }

    /// Number of `connect` calls observed overall.
    #[must_use]
    pub fn connect_calls(&self) -> usize {
        *lock(&self.connect_calls)
    }

    /// Peak number of concurrently active passes seen for an account.
    #[must_use]
    pub fn max_active(&self, account: AccountId) -> usize {
        lock(&self.max_active).get(&account).copied().unwrap_or(0)
    }

    /// Makes every write-back op targeting the IMAP `uid` fail with `kind`,
    /// simulating a poison message the write-back drain must not wedge behind.
    pub fn poison_uid(&self, uid: u32, kind: FailKind) {
        lock(&self.poison).insert(uid, kind);
    }

    /// The mailbox names the write-back executor selected, in call order.
    #[must_use]
    pub fn select_calls(&self) -> Vec<MailboxName> {
        lock(&self.select_calls).clone()
    }

    /// The `(id, delta)` of every `store_flags` call, in order.
    #[must_use]
    pub fn flag_calls(&self) -> Vec<(RemoteMessageId, FlagDelta)> {
        lock(&self.flag_calls).clone()
    }

    /// The `(id, target)` of every `move_message` call, in order.
    #[must_use]
    pub fn move_calls(&self) -> Vec<(RemoteMessageId, MailboxName)> {
        lock(&self.move_calls).clone()
    }

    /// The scripted failure for `id`'s UID, if it was poisoned.
    fn poison_for(&self, id: &RemoteMessageId) -> Option<FailKind> {
        let RemoteMessageId::Imap { uid, .. } = id else {
            return None;
        };
        lock(&self.poison).get(uid).copied()
    }

    fn should_pause(&self, current: Option<u32>) -> bool {
        matches!(*lock(&self.pause_at), Some(target) if target == current)
    }

    fn record_sync(&self, mailbox: MailboxId) {
        *lock(&self.sync_calls).entry(mailbox).or_insert(0) += 1;
    }

    fn pop_pass(&self, mailbox: MailboxId) -> Option<Pass> {
        lock(&self.plans)
            .get_mut(&mailbox)
            .and_then(VecDeque::pop_front)
    }

    fn enter(world: &Arc<Self>, account: AccountId) -> ActiveGuard {
        let current = {
            let mut active = lock(&world.active);
            let slot = active.entry(account).or_insert(0);
            *slot += 1;
            *slot
        };
        let mut max = lock(&world.max_active);
        let peak = max.entry(account).or_insert(0);
        if current > *peak {
            *peak = current;
        }
        ActiveGuard {
            world: Arc::clone(world),
            account,
        }
    }

    fn leave(&self, account: AccountId) {
        if let Some(slot) = lock(&self.active).get_mut(&account) {
            *slot = slot.saturating_sub(1);
        }
    }
}

/// Decrements the active counter when a pass ends (including on abort).
struct ActiveGuard {
    world: Arc<FakeWorld>,
    account: AccountId,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.world.leave(self.account);
    }
}

/// A scripted provider bound to one account, sharing a [`FakeWorld`].
pub struct FakeProvider {
    world: Arc<FakeWorld>,
    account: AccountId,
}

impl IncomingMailProvider for FakeProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        *lock(&self.world.connect_calls) += 1;
        match lock(&self.world.connect_errors).pop_front() {
            Some(kind) => Err(kind.build()),
            None => Ok(()),
        }
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Ok(lock(&self.world.discovery)
            .get(&self.account)
            .cloned()
            .unwrap_or(RemoteMailboxState {
                mailboxes: Vec::new(),
            }))
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        let _active = FakeWorld::enter(&self.world, self.account);
        // Copy the delay out and drop the guard before awaiting, so no
        // `MutexGuard` is held across an await point (keeps the future Send).
        let delay = *lock(&self.world.pass_delay);
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        if self.world.should_pause(request.state.highest_uid) {
            // Freeze here until the job is cancelled (guarded select) or the
            // engine is dropped (task abort). Never notified in tests.
            self.world.gate.notified().await;
        }
        self.world.record_sync(request.mailbox);
        match self.world.pop_pass(request.mailbox) {
            Some(Pass::Batch(outcome)) => Ok(outcome),
            Some(Pass::Fail(kind)) => Err(kind.build()),
            // An unscripted pass is a no-op that ends the loop.
            None => Ok(SyncOutcome {
                uid_validity: request.state.uid_validity,
                highest_uid: request.state.highest_uid,
                has_more: false,
                new_envelopes: Vec::new(),
                flag_updates: Vec::new(),
                removed: Vec::new(),
                present_remote_ids: None,
            }),
        }
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        *lock(&self.world.fetch_calls) += 1;
        match lock(&self.world.raw_messages).get(id).cloned() {
            Some(bytes) => Ok(RawMessage {
                remote_id: id.clone(),
                bytes,
            }),
            None => Err(unsupported("FETCH")),
        }
    }

    async fn select_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        lock(&self.world.select_calls).push(mailbox.clone());
        Ok(())
    }

    async fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> Result<(), MailError> {
        if let Some(kind) = self.world.poison_for(id) {
            return Err(kind.build());
        }
        lock(&self.world.flag_calls).push((id.clone(), delta.clone()));
        Ok(())
    }

    async fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        if let Some(kind) = self.world.poison_for(id) {
            return Err(kind.build());
        }
        lock(&self.world.move_calls).push((id.clone(), target.clone()));
        Ok(())
    }

    async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
        Err(unsupported("EXPUNGE"))
    }

    async fn create_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Ok(())
    }

    async fn rename_mailbox(
        &mut self,
        _from: &MailboxName,
        _to: &MailboxName,
    ) -> Result<(), MailError> {
        Ok(())
    }

    async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Ok(())
    }

    async fn set_mailbox_subscribed(
        &mut self,
        _path: &MailboxName,
        _subscribed: bool,
    ) -> Result<(), MailError> {
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

fn unsupported(capability: &str) -> MailError {
    MailError::UnsupportedCapability {
        capability: capability.to_owned(),
    }
}

/// Factory that hands out [`FakeProvider`]s sharing one [`FakeWorld`].
pub struct FakeFactory {
    world: Arc<FakeWorld>,
}

impl FakeFactory {
    /// Wraps a world in a factory.
    #[must_use]
    pub fn new(world: Arc<FakeWorld>) -> Self {
        Self { world }
    }
}

impl ProviderFactory<FakeProvider> for FakeFactory {
    async fn make(&self, account: &Account) -> Result<FakeProvider, MailError> {
        Ok(FakeProvider {
            world: Arc::clone(&self.world),
            account: account.id,
        })
    }
}

/// Locks a mutex, tolerating poisoning (tests never leave meaningful
/// invariants broken across a panic).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// Fixtures and builders
// ---------------------------------------------------------------------------

/// A fast test configuration: short buffers, real backoff timing (paused
/// clocks advance it for free), timeouts long enough never to fire on the
/// scripted, in-memory paths.
#[must_use]
pub fn test_config() -> SyncConfig {
    SyncConfig {
        max_parallel_jobs: 2,
        network_timeout: Duration::from_secs(30),
        batch_size: 100,
        offline_download_batch: 25,
        max_attempts: 4,
        backoff_base: Duration::from_secs(2),
        backoff_cap: Duration::from_secs(60),
        command_buffer: 16,
        event_buffer: 256,
    }
}

/// Opens an in-memory store with one account and one mailbox.
pub async fn store_with_mailbox() -> (Store, AccountId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store.accounts().insert(&sample_account()).await.unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    (store, account.id, mailbox.id)
}

/// Opens an in-memory store with one account and two mailboxes.
pub async fn store_with_two_mailboxes() -> (Store, AccountId, MailboxId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store.accounts().insert(&sample_account()).await.unwrap();
    let inbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    let archive = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "Archive"))
        .await
        .unwrap();
    (store, account.id, inbox.id, archive.id)
}

fn sample_account() -> NewAccount {
    let port = |value: u16| NonZeroU16::new(value).unwrap();
    NewAccount {
        display_name: "Work".to_owned(),
        email: EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        auth: AccountAuth::Password,
        incoming: IncomingServerConfig {
            protocol: IncomingProtocol::Imap,
            endpoint: ServerEndpoint::new("imap.example.org", port(993)).unwrap(),
            security: ConnectionSecurity::Tls,
            username: "klaus@theferbers.eu".to_owned(),
            credential: CredentialRef::new("axomail/account/incoming"),
        },
        outgoing: OutgoingServerConfig {
            endpoint: ServerEndpoint::new("smtp.example.org", port(465)).unwrap(),
            security: ConnectionSecurity::StartTls,
            username: "klaus@theferbers.eu".to_owned(),
            credential: CredentialRef::new("axomail/account/outgoing"),
        },
    }
}

fn sample_mailbox(account: AccountId, name: &str) -> NewMailbox {
    NewMailbox {
        account,
        name: MailboxName::new(name),
        remote_name: MailboxName::new(name),
        role: MailboxRole::guess_from_name(name),
        uid_validity: None,
        uid_next: None,
    }
}

/// Builds an IMAP envelope for `uid` under `uid_validity`.
#[must_use]
pub fn envelope(uid: u32, uid_validity: u32, subject: &str, seen: bool) -> RemoteEnvelope {
    RemoteEnvelope {
        remote_id: RemoteMessageId::Imap { uid, uid_validity },
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: Some("Ada Lovelace".to_owned()),
            email: EmailAddress::parse("ada@example.org").unwrap(),
        }],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        date: Some(UtcTimestamp::from_unix_seconds(
            1_700_000_000 + i64::from(uid),
        )),
        flags: MessageFlags {
            seen,
            ..MessageFlags::default()
        },
        size_bytes: Some(1024),
        threading: None,
    }
}

/// Builds an outcome carrying `new` envelopes and no flag/removal changes.
#[must_use]
pub fn batch(
    uid_validity: u32,
    highest_uid: u32,
    has_more: bool,
    new: Vec<RemoteEnvelope>,
) -> SyncOutcome {
    SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid: Some(highest_uid),
        has_more,
        new_envelopes: new,
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    }
}

/// Builds an outcome with flag updates and removals but no new messages.
#[must_use]
pub fn changes(
    uid_validity: u32,
    highest_uid: u32,
    flag_updates: Vec<(RemoteMessageId, MessageFlags)>,
    removed: Vec<RemoteMessageId>,
) -> SyncOutcome {
    SyncOutcome {
        uid_validity: Some(uid_validity),
        highest_uid: Some(highest_uid),
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates,
        removed,
        present_remote_ids: None,
    }
}

// ---------------------------------------------------------------------------
// Event helpers
// ---------------------------------------------------------------------------

/// Receives events until a terminal one (`JobFinished`/`JobFailed`) or a
/// safety timeout, returning everything observed. Broadcast lag is
/// skipped, not treated as terminal.
pub async fn drain_until_terminal(
    rx: &mut tokio::sync::broadcast::Receiver<SyncEvent>,
) -> Vec<SyncEvent> {
    let mut events = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Ok(event)) => {
                let terminal = matches!(
                    event,
                    SyncEvent::JobFinished { .. }
                        | SyncEvent::JobFailed { .. }
                        | SyncEvent::JobCancelled { .. }
                );
                events.push(event);
                if terminal {
                    return events;
                }
            }
            Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) => return events,
            Err(_timeout) => return events,
        }
    }
}

/// Waits for the first `BatchIngested` event and returns its
/// `new_messages` count.
pub async fn wait_for_batch(rx: &mut tokio::sync::broadcast::Receiver<SyncEvent>) -> u64 {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Ok(SyncEvent::BatchIngested { new_messages, .. })) => return new_messages,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => {
                panic!("no BatchIngested event before the channel closed or timed out")
            }
        }
    }
}

/// Extracts the job id announced by the first `JobQueued` event.
#[must_use]
pub fn queued_job_id(events: &[SyncEvent]) -> SyncJobId {
    events
        .iter()
        .find_map(|event| match event {
            SyncEvent::JobQueued { job_id, .. } => Some(*job_id),
            _ => None,
        })
        .expect("a JobQueued event was emitted")
}

/// Counts messages currently stored in a mailbox.
pub async fn message_count(store: &Store, mailbox: MailboxId) -> usize {
    store
        .messages()
        .list_page(mailbox, None, 1000)
        .await
        .unwrap()
        .items
        .len()
}
