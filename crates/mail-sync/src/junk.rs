//! Junk disposition: the local move + `$Junk` keyword + write-back ops shared
//! by the sync worker's auto-router and the mark-junk / not-junk commands.
//!
//! Both entry points follow the established write-back shape (see
//! [`op_payload`](crate::op_payload)): apply the change to local storage first
//! (optimistic — the UI reads the new location and keyword back at once), then,
//! for an IMAP message with a server identity, enqueue the `pending_ops` rows
//! (`+FLAGS $Junk`/`-FLAGS $Junk` and the move) so the change reaches the
//! server. POP3 and local-only messages are dispositioned locally with no op.
//! Triggering the `OpReplay` drain is the caller's job: the worker re-drains its
//! own queue, the command layer calls `engine.enqueue_work`.

use axomail_core::{
    AccountId, Keyword, MailError, MailboxId, MailboxRole, MessageEnvelope, MessageId,
    RemoteMessageId,
};
use axomail_storage::{NewPendingOp, NewTag, Store};

use crate::op_payload::{keyword_op, move_op, seen_op};

/// The `$Junk` IMAP keyword. Parsed on demand; the literal is a valid RFC 3501
/// atom, but the classifier never panics on a parse (clippy-clean).
fn junk_keyword() -> Result<Keyword, MailError> {
    Keyword::parse("$Junk")
}

/// Files a message as junk: moves it to the account's Junk folder, sets the
/// `$Junk` keyword, and (for IMAP) enqueues the matching server ops. Returns the
/// account whose write-back queue gained ops, or `None` when nothing was queued
/// (no Junk folder, POP3/local message, or the message is gone).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn route_to_junk(
    store: &Store,
    message_id: MessageId,
) -> Result<Option<AccountId>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };
    let Some(account) = mailbox_account(store, envelope.mailbox).await? else {
        return Ok(None);
    };
    let Some(junk) = role_mailbox(store, account, MailboxRole::Junk).await? else {
        // No Junk folder discovered yet: leave the message in place (its score
        // is still stored). "Run junk controls" can retry once folders exist.
        return Ok(None);
    };
    apply(store, &envelope, account, junk, true).await
}

/// Restores a message from junk: moves it back to the Inbox, clears the `$Junk`
/// keyword, and (for IMAP) enqueues the matching server ops. Returns the account
/// whose queue gained ops, or `None` when nothing was queued.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn route_from_junk(
    store: &Store,
    message_id: MessageId,
) -> Result<Option<AccountId>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };
    let Some(account) = mailbox_account(store, envelope.mailbox).await? else {
        return Ok(None);
    };
    // Prefer the Inbox as the restore target; if the account somehow has none,
    // just clear the keyword where the message sits.
    let target = role_mailbox(store, account, MailboxRole::Inbox)
        .await?
        .unwrap_or(envelope.mailbox);
    apply(store, &envelope, account, target, false).await
}

/// Applies the local disposition (keyword toggle + optional move) and enqueues
/// the server ops for an IMAP message. `add_junk` chooses set-vs-clear `$Junk`.
async fn apply(
    store: &Store,
    envelope: &MessageEnvelope,
    account: AccountId,
    target: MailboxId,
    add_junk: bool,
) -> Result<Option<AccountId>, MailError> {
    let source = envelope.mailbox;
    let keyword = junk_keyword()?;
    set_local_keyword(store, account, envelope.id, &keyword, add_junk).await?;

    let moved = source != target;
    if moved {
        store.messages().set_mailbox(envelope.id, target).await?;
    }

    // Optionally mark the message read when filing it to Junk (the
    // "mark-read-on-junk" setting). Only applies when *adding* to junk, and only
    // for a message not already seen, so restoring from junk never touches it.
    let mark_read =
        add_junk && !envelope.flags.seen && store.junk().settings().await?.mark_read_on_junk;
    if mark_read {
        let mut flags = envelope.flags.clone();
        flags.seen = true;
        store.messages().update_flags(envelope.id, &flags).await?;
    }

    // Only an IMAP message with a server identity propagates; the enqueued ops
    // wait for an OpReplay the caller triggers.
    if !matches!(envelope.remote_id, Some(RemoteMessageId::Imap { .. })) {
        return Ok(None);
    }
    let (keyword_kind, keyword_payload) = keyword_op(keyword, add_junk);
    enqueue(store, account, envelope.id, keyword_kind, keyword_payload).await?;
    if moved {
        let (move_kind, move_payload) = move_op(source, target);
        enqueue(store, account, envelope.id, move_kind, move_payload).await?;
    }
    if mark_read {
        let (seen_kind, seen_payload) = seen_op(true);
        enqueue(store, account, envelope.id, seen_kind, seen_payload).await?;
    }
    Ok(Some(account))
}

/// Enqueues one write-back op.
async fn enqueue(
    store: &Store,
    account_id: AccountId,
    message_id: MessageId,
    kind: axomail_storage::PendingOpKind,
    payload_json: String,
) -> Result<(), MailError> {
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id,
            message_id,
            kind,
            payload_json,
        })
        .await?;
    Ok(())
}

/// Adds or removes a message's local keyword tag, auto-creating the account tag
/// on first use (so the reading pane shows `$Junk` immediately).
async fn set_local_keyword(
    store: &Store,
    account: AccountId,
    message_id: MessageId,
    keyword: &Keyword,
    add: bool,
) -> Result<(), MailError> {
    if add {
        let tag = match store.tags().find_by_keyword(account, keyword).await? {
            Some(tag) => tag,
            None => {
                let sort_order =
                    i64::try_from(store.tags().list(account).await?.len()).unwrap_or(0);
                store
                    .tags()
                    .create(&NewTag {
                        account,
                        keyword: keyword.clone(),
                        display_name: "Junk".to_owned(),
                        color: None,
                        sort_order,
                    })
                    .await?
            }
        };
        store.tags().add_message_tag(message_id, tag.id).await?;
    } else if let Some(tag) = store.tags().find_by_keyword(account, keyword).await? {
        store.tags().remove_message_tag(message_id, tag.id).await?;
    }
    Ok(())
}

/// The owning account of a mailbox, or `None` if the mailbox is gone.
async fn mailbox_account(
    store: &Store,
    mailbox: MailboxId,
) -> Result<Option<AccountId>, MailError> {
    Ok(store.mailboxes().get(mailbox).await?.map(|mb| mb.account))
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
