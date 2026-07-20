//! The filter action runner: evaluate an account's rules against a set of
//! messages and apply the resulting actions.
//!
//! This is the single place that turns evaluated [`FilterAction`]s into real
//! effects, reusing the exact write-back primitives the command layer uses — the
//! optimistic local change plus, for a write-back-capable IMAP message, a
//! `pending_ops` row built with the shared [`op_payload`](crate::op_payload)
//! builders. It never re-implements move/copy/tag/flag/delete semantics; it
//! composes them. The runner does **not** trigger the replay: it returns the set
//! of accounts whose write-back queue gained an op, and the caller (the manual
//! command or the arrival hook) enqueues one `OpReplay` per account.
//!
//! Both entry points — run-on-arrival (post-ingest, see [`crate::worker`]) and
//! the manual "run on folder" command — funnel through
//! [`run_filters_on_messages`].

use std::collections::BTreeSet;

use axomail_core::{
    AccountId, Address, EmailAddress, IncomingProtocol, MailError, MailboxId, MailboxRole,
    MessageEnvelope, MessageFlags, MessageId,
};
use axomail_storage::{
    ActionKind, FilterAction, FilterProgram, NewMessage, NewOutbox, NewPendingOp, PendingOpKind,
    Store,
};

use crate::op_payload::{copy_op, delete_op, flagged_op, keyword_op, move_op, seen_op};

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

/// Evaluates `account`'s active filters against each message in `message_ids`
/// (in order) and applies the matched actions.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any storage failure.
pub async fn run_filters_on_messages(
    store: &Store,
    account_id: AccountId,
    message_ids: &[MessageId],
) -> Result<FilterRunReport, MailError> {
    let mut report = FilterRunReport::default();
    let filters = store.filters().active_for_account(account_id).await?;
    let program = FilterProgram::compile(&filters);
    if program.is_empty() {
        return Ok(report);
    }
    let Some(account) = store.accounts().get(account_id).await? else {
        return Ok(report);
    };
    let is_imap = account.incoming.protocol == IncomingProtocol::Imap;
    let trash = trash_mailbox(store, account_id).await?;

    for &message_id in message_ids {
        let Some(facts) = store.filters().facts_for_message(message_id).await? else {
            continue;
        };
        let actions = program.evaluate(&facts);
        if actions.is_empty() {
            continue;
        }
        report.matched += 1;
        apply_actions(
            store,
            account_id,
            is_imap,
            trash,
            message_id,
            &actions,
            &mut report,
        )
        .await?;
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

/// The fixed context for applying one message's actions: the store, the
/// account, the target message, and whether server write-back applies.
struct MessageCtx<'a> {
    store: &'a Store,
    account_id: AccountId,
    message_id: MessageId,
    writeback: bool,
}

/// Applies one message's ordered actions, tracking its live mailbox and flags
/// as they change so later actions in the same run see the current state.
async fn apply_actions(
    store: &Store,
    account_id: AccountId,
    is_imap: bool,
    trash: Option<MailboxId>,
    message_id: MessageId,
    actions: &[FilterAction],
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(());
    };
    let ctx = MessageCtx {
        store,
        account_id,
        message_id,
        // Write-back applies only to an IMAP message with a server identity.
        writeback: is_imap && envelope.remote_id.is_some(),
    };
    let mut source = envelope.mailbox;
    let mut flags = envelope.flags.clone();

    for action in actions {
        match action.kind {
            ActionKind::Stop => break,
            ActionKind::Move => {
                if let Some(target) = resolve_target(store, account_id, &action.param).await?
                    && target != source
                {
                    relocate(&ctx, source, target, false, report).await?;
                    source = target;
                }
            }
            ActionKind::DeleteToTrash => {
                if let Some(target) = trash
                    && target != source
                {
                    relocate(&ctx, source, target, true, report).await?;
                    source = target;
                }
            }
            ActionKind::Copy => {
                if let Some(target) = resolve_target(store, account_id, &action.param).await? {
                    copy(&ctx, &envelope, source, target, report).await?;
                }
            }
            ActionKind::Tag => {
                apply_tag(&ctx, action.param.trim(), report).await?;
            }
            ActionKind::MarkJunk => {
                apply_tag(&ctx, "$Junk", report).await?;
            }
            ActionKind::Star => {
                set_flag(&ctx, &mut flags, Flag::Flagged, true, report).await?;
            }
            ActionKind::MarkRead => {
                set_flag(&ctx, &mut flags, Flag::Seen, true, report).await?;
            }
            ActionKind::MarkUnread => {
                set_flag(&ctx, &mut flags, Flag::Seen, false, report).await?;
            }
            ActionKind::Forward => {
                forward(&ctx, &envelope, action.param.trim(), report).await?;
            }
            // No local priority store exists yet (no schema for it); accepted
            // and preserved in the rule, applied as a no-op. Documented.
            ActionKind::SetPriority => {}
        }
    }
    Ok(())
}

/// Resolves a move/copy target mailbox id from an action param, keeping it only
/// when it belongs to the running account.
async fn resolve_target(
    store: &Store,
    account_id: AccountId,
    param: &str,
) -> Result<Option<MailboxId>, MailError> {
    let Some(id) = param.trim().parse::<i64>().ok().map(MailboxId::new) else {
        return Ok(None);
    };
    Ok(store
        .mailboxes()
        .get(id)
        .await?
        .filter(|mailbox| mailbox.account == account_id)
        .map(|mailbox| mailbox.id))
}

/// Optimistically relocates a message and, for a write-back message, enqueues
/// the matching `move`/`delete` op (source captured before the local move).
async fn relocate(
    ctx: &MessageCtx<'_>,
    source: MailboxId,
    target: MailboxId,
    delete: bool,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    ctx.store
        .messages()
        .set_mailbox(ctx.message_id, target)
        .await?;
    if ctx.writeback {
        let (kind, payload_json) = if delete {
            delete_op(source)
        } else {
            move_op(source, target)
        };
        enqueue(ctx, kind, payload_json, report).await?;
    }
    report.actions_applied += 1;
    Ok(())
}

/// Copies a message: an IMAP write-back enqueues a server copy op; a local /
/// POP3 message is duplicated as a new local row (without the raw body — a
/// documented limitation of a filter copy).
async fn copy(
    ctx: &MessageCtx<'_>,
    envelope: &MessageEnvelope,
    source: MailboxId,
    target: MailboxId,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    if ctx.writeback {
        let (kind, payload_json) = copy_op(source, target);
        enqueue(ctx, kind, payload_json, report).await?;
    } else {
        let mut new_flags = envelope.flags.clone();
        new_flags.deleted = false;
        ctx.store
            .messages()
            .insert(&NewMessage {
                mailbox: target,
                remote_id: None,
                subject: envelope.subject.clone(),
                from: envelope.from.clone(),
                to: envelope.to.clone(),
                cc: Vec::new(),
                bcc: Vec::new(),
                date: envelope.date,
                flags: new_flags,
                size_bytes: envelope.size_bytes,
                preview: envelope.preview.clone(),
                has_raw: false,
            })
            .await?;
    }
    report.actions_applied += 1;
    Ok(())
}

/// Applies a tag by name (creating it if needed) and, for a write-back message,
/// enqueues the additive keyword op — but only when the tag was newly added.
async fn apply_tag(
    ctx: &MessageCtx<'_>,
    name: &str,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    if name.is_empty() {
        return Ok(());
    }
    let tag = ctx
        .store
        .tags()
        .ensure_by_name(ctx.account_id, name)
        .await?;
    let added = ctx
        .store
        .tags()
        .add_message_tag(ctx.message_id, tag.id)
        .await?;
    if added && ctx.writeback {
        let (kind, payload_json) = keyword_op(tag.keyword.clone(), true);
        enqueue(ctx, kind, payload_json, report).await?;
    }
    report.actions_applied += 1;
    Ok(())
}

/// A system flag a filter can set.
#[derive(Clone, Copy)]
enum Flag {
    Seen,
    Flagged,
}

/// Sets a system flag locally and, for a write-back message whose value
/// actually changed, enqueues the flag op.
async fn set_flag(
    ctx: &MessageCtx<'_>,
    flags: &mut MessageFlags,
    flag: Flag,
    value: bool,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    let current = match flag {
        Flag::Seen => flags.seen,
        Flag::Flagged => flags.flagged,
    };
    if current == value {
        return Ok(());
    }
    match flag {
        Flag::Seen => flags.seen = value,
        Flag::Flagged => flags.flagged = value,
    }
    ctx.store
        .messages()
        .update_flags(ctx.message_id, flags)
        .await?;
    if ctx.writeback {
        let (kind, payload_json) = match flag {
            Flag::Seen => seen_op(value),
            Flag::Flagged => flagged_op(value),
        };
        enqueue(ctx, kind, payload_json, report).await?;
    }
    report.actions_applied += 1;
    Ok(())
}

/// Queues a plain-text forward of the message to `address` through the outbox
/// (drained by the existing outbox worker). An invalid address is skipped.
async fn forward(
    ctx: &MessageCtx<'_>,
    envelope: &MessageEnvelope,
    address: &str,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    if EmailAddress::parse(address).is_err() {
        return Ok(());
    }
    let subject = format!(
        "Fwd: {}",
        envelope.subject.as_deref().unwrap_or("(no subject)")
    );
    let body = format!(
        "---------- Forwarded message ----------\nFrom: {}\nSubject: {}\n\n{}",
        render_from(&envelope.from),
        envelope.subject.as_deref().unwrap_or("(no subject)"),
        envelope.preview.as_deref().unwrap_or_default(),
    );
    ctx.store
        .outbox()
        .create(&NewOutbox {
            account: ctx.account_id,
            draft_id: None,
            identity_id: None,
            subject,
            body_text: body,
            to: vec![address.to_owned()],
            cc: Vec::new(),
            bcc: Vec::new(),
            next_attempt_unix: None,
        })
        .await?;
    report.forwards_queued += 1;
    report.actions_applied += 1;
    Ok(())
}

/// Renders the first sender address for a forward header.
fn render_from(from: &[Address]) -> String {
    match from.first() {
        Some(address) => match &address.display_name {
            Some(name) => format!("{name} <{}>", address.email.as_str()),
            None => address.email.as_str().to_owned(),
        },
        None => "(unknown sender)".to_owned(),
    }
}

/// Enqueues a write-back op and records the account for a later replay.
async fn enqueue(
    ctx: &MessageCtx<'_>,
    kind: PendingOpKind,
    payload_json: String,
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    ctx.store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: ctx.account_id,
            message_id: ctx.message_id,
            kind,
            payload_json,
        })
        .await?;
    report.replay_accounts.insert(ctx.account_id);
    Ok(())
}
