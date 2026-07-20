//! Applying one message's evaluated filter actions to storage.
//!
//! This is the single place that turns evaluated [`FilterAction`]s into real
//! effects, reusing the exact write-back primitives the command layer uses — the
//! optimistic local change plus, for a write-back-capable IMAP message, a
//! `pending_ops` row built with the shared [`op_payload`](crate::op_payload)
//! builders. It never re-implements move/copy/tag/flag/delete semantics; it
//! composes them.

use std::path::{Path, PathBuf};

use axomail_core::{
    AccountId, Address, EmailAddress, MailError, MailboxId, MessageEnvelope, MessageFlags,
    MessageId,
};
use axomail_parser::parse_message;
use axomail_storage::{
    ActionKind, FilterAction, NewMessage, NewOutbox, NewPendingOp, PendingOpKind, Store,
    raw_message_path, read_raw, write_raw_atomically,
};

use super::FilterRunReport;
use crate::op_payload::{copy_op, delete_op, flagged_op, keyword_op, move_op, seen_op};

/// The per-account context for a filter run: the account, whether server
/// write-back applies (IMAP), the account's Trash mailbox, and the raw-store
/// root (for copying / forwarding a full body). Built once per run.
pub(super) struct FilterContext {
    pub(super) account_id: AccountId,
    pub(super) is_imap: bool,
    pub(super) trash: Option<MailboxId>,
    pub(super) raw_root: Option<PathBuf>,
}

/// The fixed context for applying one message's actions.
struct MessageCtx<'a> {
    store: &'a Store,
    account_id: AccountId,
    message_id: MessageId,
    writeback: bool,
    raw_root: Option<&'a Path>,
}

/// Applies one message's ordered actions, tracking its live mailbox and flags
/// as they change so later actions in the same run see the current state.
pub(super) async fn apply_message(
    store: &Store,
    context: &FilterContext,
    message_id: MessageId,
    actions: &[FilterAction],
    report: &mut FilterRunReport,
) -> Result<(), MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(());
    };
    let ctx = MessageCtx {
        store,
        account_id: context.account_id,
        message_id,
        // Write-back applies only to an IMAP message with a server identity.
        writeback: context.is_imap && envelope.remote_id.is_some(),
        raw_root: context.raw_root.as_deref(),
    };
    let mut source = envelope.mailbox;
    let mut flags = envelope.flags.clone();

    for action in actions {
        match action.kind {
            ActionKind::Stop => break,
            ActionKind::Move => {
                if let Some(target) =
                    resolve_target(store, context.account_id, &action.param).await?
                    && target != source
                {
                    relocate(&ctx, source, target, false, report).await?;
                    source = target;
                }
            }
            ActionKind::DeleteToTrash => {
                if let Some(target) = context.trash
                    && target != source
                {
                    relocate(&ctx, source, target, true, report).await?;
                    source = target;
                }
            }
            ActionKind::Copy => {
                if let Some(target) =
                    resolve_target(store, context.account_id, &action.param).await?
                {
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
/// POP3 message is duplicated as a new local row — carrying the source's cached
/// raw body (when the offline store holds it) so the copy is displayable and
/// exportable, not a body-less envelope.
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
        // Read the source raw first so the new row's `has_raw` is honest: it is
        // set only after the copied blob has been written (mirrors the atomic
        // download path; never claim a cached body that does not exist).
        let raw = load_source_raw(ctx, envelope).await?;
        let mut new_flags = envelope.flags.clone();
        new_flags.deleted = false;
        let new = ctx
            .store
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
        if let Some((raw_root, bytes)) = raw {
            let path = raw_message_path(raw_root, new.id.value());
            write_raw_atomically(&path, &bytes).await?;
            // Flip `has_raw` and index the copied body for search, atomically.
            let body = parse_message(&bytes)
                .ok()
                .and_then(|parsed| parsed.text_body)
                .unwrap_or_default();
            ctx.store.offline().mark_downloaded(new.id, &body).await?;
        }
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
/// (drained by the existing outbox worker). The **full** body is forwarded when
/// the raw is cached in the offline store; otherwise it falls back to the
/// preview snippet (the runner has no provider to fetch on demand — a documented
/// best-effort). An invalid address is skipped.
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
    let full_body = match load_source_raw(ctx, envelope).await? {
        Some((_, bytes)) => parse_message(&bytes)
            .ok()
            .and_then(|parsed| parsed.text_body),
        None => None,
    };
    let body_text = full_body.unwrap_or_else(|| envelope.preview.clone().unwrap_or_default());
    let body = format!(
        "---------- Forwarded message ----------\nFrom: {}\nSubject: {}\n\n{body_text}",
        render_from(&envelope.from),
        envelope.subject.as_deref().unwrap_or("(no subject)"),
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

/// Reads a message's cached raw body from the offline store, returning the raw
/// root alongside the bytes so a copy can write the duplicate blob. `None` when
/// the offline store is disabled or the message has no cached raw.
async fn load_source_raw<'a>(
    ctx: &MessageCtx<'a>,
    envelope: &MessageEnvelope,
) -> Result<Option<(&'a Path, Vec<u8>)>, MailError> {
    let Some(raw_root) = ctx.raw_root else {
        return Ok(None);
    };
    let path = raw_message_path(raw_root, envelope.id.value());
    Ok(read_raw(&path).await?.map(|bytes| (raw_root, bytes)))
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
