//! The unit of work accepted by the engine.

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
