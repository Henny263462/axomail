//! Copy — duplicate a message into another folder. IMAP enqueues a server copy
//! op (the copy appears in the target on its next sync); POP3/local messages
//! are duplicated as a new local row (with the raw body when cached). Copy is
//! not undoable.

use std::collections::BTreeSet;
use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError, MailboxId, MessageEnvelope, MessageId};
use axomail_storage::{
    NewMessage, NewPendingOp, Store, raw_message_path, read_raw, write_raw_atomically,
};
use axomail_sync::{ProviderFactory, SyncEngine, copy_op};

use super::{config, message_account, trigger_replays, writeback_account};

/// Copies a selection into `target_mailbox_id`. Cross-account messages are
/// skipped (a target is one account's folder).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown target folder and
/// [`MailError::Database`] / [`MailError::Filesystem`] on failure.
pub async fn copy_messages_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    raw_root: &Path,
    message_ids: Vec<i64>,
    target_mailbox_id: i64,
) -> Result<(), MailError>
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
    let mut accounts = BTreeSet::new();
    for id in message_ids {
        let message_id = MessageId::new(id);
        let Some((envelope, account)) = message_account(store, message_id).await? else {
            continue;
        };
        if account != target_mailbox.account {
            continue;
        }
        if let Some(account_id) = writeback_account(store, &envelope, envelope.mailbox).await? {
            let (kind, payload_json) = copy_op(envelope.mailbox, target);
            store
                .pending_ops()
                .enqueue(&NewPendingOp {
                    account_id,
                    message_id,
                    kind,
                    payload_json,
                })
                .await?;
            accounts.insert(account_id);
        } else {
            duplicate_local(store, raw_root, &envelope, target).await?;
        }
    }
    trigger_replays(engine, accounts).await
}

/// Duplicates a message as a new local-only row in `target` (the POP3/local
/// copy path), carrying the raw body over when it is cached so the copy opens
/// offline. The `\Deleted` flag is cleared; `cc`/`bcc` are not carried (they
/// are absent from the list envelope) — a documented limitation of a local
/// copy.
async fn duplicate_local(
    store: &Store,
    raw_root: &Path,
    envelope: &MessageEnvelope,
    target: MailboxId,
) -> Result<(), MailError> {
    let raw = read_raw(&raw_message_path(raw_root, envelope.id.value())).await?;
    let mut flags = envelope.flags.clone();
    flags.deleted = false;
    let new = NewMessage {
        mailbox: target,
        remote_id: None,
        subject: envelope.subject.clone(),
        from: envelope.from.clone(),
        to: envelope.to.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        date: envelope.date,
        flags,
        size_bytes: envelope.size_bytes,
        preview: envelope.preview.clone(),
        has_raw: raw.is_some(),
    };
    let inserted = store.messages().insert(&new).await?;
    if let Some(bytes) = raw {
        write_raw_atomically(&raw_message_path(raw_root, inserted.id.value()), &bytes).await?;
    }
    Ok(())
}
