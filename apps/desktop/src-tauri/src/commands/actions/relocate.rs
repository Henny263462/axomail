//! Move / archive / delete-to-Trash / undo — the relocation commands that
//! optimistically move messages locally and enqueue a `move`/`delete` op.

use std::collections::{BTreeSet, HashMap};

use axomail_core::{AccountId, IncomingMailProvider, MailError, MailboxId, MailboxRole, MessageId};
use axomail_storage::Store;
use axomail_sync::{ProviderFactory, SyncEngine};

use crate::dto::{MessageMoveDto, MoveOutcomeDto};

use super::{config, message_account, relocate_one, trigger_replays};

/// Runs a relocation over a set of ids where the target for each message is
/// resolved from its account by `targets`, gathering undo records + replay
/// accounts into a [`MoveOutcomeDto`].
async fn relocate_set<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    ids: Vec<i64>,
    targets: &HashMap<AccountId, MailboxId>,
    delete: bool,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut moves = Vec::new();
    let mut accounts = BTreeSet::new();
    for id in ids {
        let message_id = MessageId::new(id);
        let Some((_, account)) = message_account(store, message_id).await? else {
            continue;
        };
        let Some(&target) = targets.get(&account) else {
            continue;
        };
        if let Some((source, op_account)) = relocate_one(store, message_id, target, delete).await? {
            moves.push(MessageMoveDto::new(id, source.value())?);
            if let Some(account_id) = op_account {
                accounts.insert(account_id);
            }
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(MoveOutcomeDto { moves })
}

/// Which well-known destination a bulk relocation resolves per account.
#[derive(Clone, Copy)]
pub(super) enum TargetRole {
    Trash,
    Archive,
}

/// Resolves each involved account's Trash or Archive destination, erroring if
/// any account lacks one. Pre-flighting the whole set keeps a missing folder
/// from moving *some* messages before it is discovered. The Archive variant
/// resolves the account's *base* archive; the granularity sub-foldering lives in
/// [`super::archive`].
pub(super) async fn resolve_targets(
    store: &Store,
    ids: &[i64],
    role: TargetRole,
) -> Result<HashMap<AccountId, MailboxId>, MailError> {
    let mut targets = HashMap::new();
    for &id in ids {
        let Some((_, account)) = message_account(store, MessageId::new(id)).await? else {
            continue;
        };
        if targets.contains_key(&account) {
            continue;
        }
        let target = match role {
            TargetRole::Trash => role_mailbox(store, account, MailboxRole::Trash)
                .await?
                .ok_or_else(|| config("this account has no Trash folder; discover folders first"))?,
            TargetRole::Archive => resolve_archive_target(store, account).await?.ok_or_else(
                || {
                    config(
                        "no Archive folder is set for this account; choose one in the identity settings or discover folders",
                    )
                },
            )?,
        };
        targets.insert(account, target);
    }
    Ok(targets)
}

/// The account's mailbox with `role`, if any.
async fn role_mailbox(
    store: &Store,
    account: AccountId,
    role: MailboxRole,
) -> Result<Option<MailboxId>, MailError> {
    Ok(store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.role == role)
        .map(|mailbox| mailbox.id))
}

/// Resolves an account's archive destination: the default identity's
/// `archive_dest` when set (and still valid for the account), else the
/// `Archive`-role mailbox.
async fn resolve_archive_target(
    store: &Store,
    account: AccountId,
) -> Result<Option<MailboxId>, MailError> {
    let identities = store.identities().list(account).await?;
    if let Some(dest) = identities
        .iter()
        .find(|identity| identity.is_default)
        .and_then(|identity| identity.fields.archive_dest)
        && let Some(mailbox) = store.mailboxes().get(dest).await?
        && mailbox.account == account
    {
        return Ok(Some(dest));
    }
    role_mailbox(store, account, MailboxRole::Archive).await
}

/// Moves a selection of messages into `target_mailbox_id`. Messages in a
/// different account than the target are skipped (a target is one account's
/// folder).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown target folder and
/// [`MailError::Database`] on any query failure.
pub async fn move_messages_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_ids: Vec<i64>,
    target_mailbox_id: i64,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let target = MailboxId::new(target_mailbox_id);
    let target_mailbox = store
        .mailboxes()
        .get(target)
        .await?
        .ok_or_else(|| config("the target folder does not exist"))?;
    // Every message in the target's account maps to the one chosen target.
    let mut targets = HashMap::new();
    targets.insert(target_mailbox.account, target);
    relocate_set(store, engine, message_ids, &targets, false).await
}

/// Deletes a selection to Trash (never a hard delete), each message into its
/// account's `Trash`-role folder.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if any involved account has no Trash
/// folder, and [`MailError::Database`] on any query failure.
pub async fn delete_to_trash_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let targets = resolve_targets(store, &message_ids, TargetRole::Trash).await?;
    relocate_set(store, engine, message_ids, &targets, true).await
}

/// Undoes a prior move/archive/delete by moving each message back to its
/// recorded source mailbox (a compensating move + op). Not for copy.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn undo_move_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    moves: Vec<MessageMoveDto>,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut accounts = BTreeSet::new();
    for record in moves {
        let target = MailboxId::new(record.from_mailbox_id);
        // Skip a vanished original mailbox rather than trip the FK.
        if store.mailboxes().get(target).await?.is_none() {
            continue;
        }
        if let Some((_, op_account)) =
            relocate_one(store, MessageId::new(record.message_id), target, false).await?
            && let Some(account_id) = op_account
        {
            accounts.insert(account_id);
        }
    }
    trigger_replays(engine, accounts).await
}
