//! The units of work accepted by the engine.

use axomail_core::{AccountId, MailboxId};

/// A request to synchronize one mailbox of one account.
///
/// Enqueuing the same mailbox again while a job for it is still pending or
/// running is de-duplicated by the engine, so callers may enqueue freely
/// (e.g. on a timer or a user refresh) without piling up work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyncMailbox {
    /// Account that owns the mailbox.
    pub account_id: AccountId,
    /// Mailbox to synchronize.
    pub mailbox_id: MailboxId,
}

/// A unit of work enqueued on the engine channel, spanning every job kind.
///
/// The dispatcher persists each request as a `sync_jobs` row (de-duplicated
/// per its dedup key) and routes the claimed job to the matching per-kind
/// executor. `MailboxSync` is mailbox-scoped; `Discover` and `OpReplay` are
/// account-scoped (the persisted row's `mailbox_id` is NULL), which is why
/// the dedup key includes the account (see
/// [`create_if_absent`](axomail_storage::SyncJobRepo::create_if_absent)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkRequest {
    /// Incrementally synchronize one mailbox.
    MailboxSync(SyncMailbox),
    /// Discover and reconcile an account's remote folder structure.
    ///
    /// Executor seam owned by Wave 2 track 2.2 (Folders).
    Discover {
        /// Account whose folders to discover.
        account_id: AccountId,
    },
    /// Replay an account's queued offline operations to the server.
    ///
    /// Executor seam owned by Wave 2 track 2.1 (Write-back).
    OpReplay {
        /// Account whose pending ops to replay.
        account_id: AccountId,
    },
}

impl WorkRequest {
    /// The account this work belongs to (all kinds are account-scoped).
    #[must_use]
    pub fn account_id(&self) -> AccountId {
        match self {
            Self::MailboxSync(mailbox) => mailbox.account_id,
            Self::Discover { account_id } | Self::OpReplay { account_id } => *account_id,
        }
    }
}

impl From<SyncMailbox> for WorkRequest {
    fn from(mailbox: SyncMailbox) -> Self {
        Self::MailboxSync(mailbox)
    }
}
