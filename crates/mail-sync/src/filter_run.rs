//! The filter action runner: evaluate an account's rules against a set of
//! messages and apply the resulting actions.
//!
//! [`FilterRunner`] compiles an account's active filters and its per-account
//! context (write-back capability, Trash target, raw-store root) **once**, then
//! applies them to messages one at a time via [`FilterRunner::apply`]. Both
//! entry points — run-on-arrival (post-ingest, see [`crate::worker`]) and the
//! manual "run on folder" command — build a runner; the arrival hook drives it
//! message-by-message so a mid-batch failure advances the high-water precisely
//! (never re-forwarding an already-forwarded message). The convenience
//! [`run_filters_on_messages`] runs a whole slice for the manual command.
//!
//! The runner does **not** trigger the replay: it records the set of accounts
//! whose write-back queue gained an op, and the caller enqueues one `OpReplay`
//! per account.

mod actions;

use std::collections::BTreeSet;
use std::path::Path;

use axomail_core::{AccountId, IncomingProtocol, MailError, MailboxId, MailboxRole, MessageId};
use axomail_storage::{FilterProgram, Store};

use actions::{FilterContext, apply_message};

/// What a filter run changed, for reporting and for the caller's replay trigger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterRunReport {
    /// Messages that matched at least one filter.
    pub matched: u64,
    /// Individual actions that produced an effect.
    pub actions_applied: u64,
    /// Forward actions queued to the outbox.
    pub forwards_queued: u64,
    /// Accounts whose write-back queue gained an op (the caller triggers a
    /// replay for each).
    pub replay_accounts: BTreeSet<AccountId>,
}

/// A loaded, compiled filter run for one account: the compiled program plus the
/// per-account context. Built once, then applied message-by-message so a
/// mid-batch failure leaves a precise high-water advance.
pub struct FilterRunner<'a> {
    store: &'a Store,
    program: FilterProgram,
    context: FilterContext,
}

impl<'a> FilterRunner<'a> {
    /// Compiles `account_id`'s active filters and resolves its run context.
    /// Returns `None` (nothing to do) when the account has no active filters or
    /// no longer exists. `raw_root` is the offline raw-store root, used to copy
    /// or forward a full body.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any storage failure.
    pub async fn load(
        store: &'a Store,
        account_id: AccountId,
        raw_root: Option<&Path>,
    ) -> Result<Option<Self>, MailError> {
        let filters = store.filters().active_for_account(account_id).await?;
        let program = FilterProgram::compile(&filters);
        if program.is_empty() {
            return Ok(None);
        }
        let Some(account) = store.accounts().get(account_id).await? else {
            return Ok(None);
        };
        let context = FilterContext {
            account_id,
            is_imap: account.incoming.protocol == IncomingProtocol::Imap,
            trash: trash_mailbox(store, account_id).await?,
            raw_root: raw_root.map(Path::to_path_buf),
        };
        Ok(Some(Self {
            store,
            program,
            context,
        }))
    }

    /// Evaluates the program against one message and applies the matched
    /// actions, accumulating effects into `report`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any storage failure.
    pub async fn apply(
        &self,
        message_id: MessageId,
        report: &mut FilterRunReport,
    ) -> Result<(), MailError> {
        let Some(facts) = self.store.filters().facts_for_message(message_id).await? else {
            return Ok(());
        };
        let actions = self.program.evaluate(&facts);
        if actions.is_empty() {
            return Ok(());
        }
        report.matched += 1;
        apply_message(self.store, &self.context, message_id, &actions, report).await
    }
}

/// Evaluates `account`'s active filters against each message in `message_ids`
/// (in order) and applies the matched actions.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any storage failure.
pub async fn run_filters_on_messages(
    store: &Store,
    account_id: AccountId,
    raw_root: Option<&Path>,
    message_ids: &[MessageId],
) -> Result<FilterRunReport, MailError> {
    let mut report = FilterRunReport::default();
    let Some(runner) = FilterRunner::load(store, account_id, raw_root).await? else {
        return Ok(report);
    };
    for &message_id in message_ids {
        runner.apply(message_id, &mut report).await?;
    }
    Ok(report)
}

/// The account's `Trash`-role mailbox, if any (for `delete_to_trash`).
async fn trash_mailbox(store: &Store, account: AccountId) -> Result<Option<MailboxId>, MailError> {
    Ok(store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Trash)
        .map(|mailbox| mailbox.id))
}
