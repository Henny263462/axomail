//! Minimal keyword op-replay executor (Wave 2 track 2.4).
//!
//! Track 2.4 (Tags) enqueues `add_keyword` / `remove_keyword`
//! [`PendingOp`](axomail_storage::PendingOp)s when the user tags or untags a
//! message. This module turns one such op into the additive server write: it
//! resolves the message's remote identity, builds a single-keyword
//! [`FlagDelta`], and issues [`store_flags`](IncomingMailProvider::store_flags),
//! which the IMAP adapter sends as `UID STORE +FLAGS`/`-FLAGS` — so a keyword
//! the client never named is never disturbed.
//!
//! This is deliberately *only* the keyword slice of op replay. It is **not**
//! wired into the engine's [`JobKind::OpReplay`](axomail_storage::JobKind)
//! dispatch seam (that stays the Stage-0 placeholder), because the full
//! pending-op drain — claim a batch, apply flag/move/delete ops, and
//! `complete`/`retry`/`fail` per outcome — is owned by track 2.1 (Write-back).
//! The integrator folds this keyword mapping into 2.1's drain, deduping with
//! its flag executor; until then it stands alone and fully tested.

use axomail_core::{FlagDelta, FlagSet, IncomingMailProvider, MailError, RemoteMessageId};
use axomail_storage::{KeywordOpPayload, PendingOp, PendingOpKind, Store};

/// Outcome of attempting to replay one pending op as a keyword write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordReplayOutcome {
    /// The keyword change was issued to the server.
    Applied,
    /// The op is not a keyword op (`add_keyword`/`remove_keyword`); a caller
    /// draining a mixed queue should handle it with another executor.
    NotKeywordOp,
    /// The op's message has no IMAP remote identity (a local-only or POP3
    /// message), so there is nothing to write to a server; treat as complete.
    NoRemoteTarget,
}

/// Replays one `add_keyword` / `remove_keyword` pending op against `provider`.
///
/// Returns [`KeywordReplayOutcome::NotKeywordOp`] for any other kind (without
/// touching the network), and [`KeywordReplayOutcome::NoRemoteTarget`] when the
/// message is missing or not an IMAP message.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the payload is malformed, and
/// propagates any [`MailError`] from the store or the provider write.
pub async fn replay_keyword_op<P>(
    store: &Store,
    provider: &mut P,
    op: &PendingOp,
) -> Result<KeywordReplayOutcome, MailError>
where
    P: IncomingMailProvider,
{
    let add = match op.kind {
        PendingOpKind::AddKeyword => true,
        PendingOpKind::RemoveKeyword => false,
        _ => return Ok(KeywordReplayOutcome::NotKeywordOp),
    };

    let payload = KeywordOpPayload::from_json(&op.payload_json)?;

    let Some(remote_id) = imap_remote_id(store, op).await? else {
        return Ok(KeywordReplayOutcome::NoRemoteTarget);
    };

    let side = FlagSet::from_keyword(payload.keyword);
    let delta = if add {
        FlagDelta {
            add: side,
            remove: FlagSet::default(),
        }
    } else {
        FlagDelta {
            add: FlagSet::default(),
            remove: side,
        }
    };

    provider.store_flags(&remote_id, &delta).await?;
    Ok(KeywordReplayOutcome::Applied)
}

/// Resolves the message's IMAP remote identity, or `None` when the message is
/// absent, local-only, or a POP3 message (no server keyword target).
async fn imap_remote_id(
    store: &Store,
    op: &PendingOp,
) -> Result<Option<RemoteMessageId>, MailError> {
    let Some(envelope) = store.messages().get(op.message_id).await? else {
        return Ok(None);
    };
    Ok(match envelope.remote_id {
        Some(id @ RemoteMessageId::Imap { .. }) => Some(id),
        Some(RemoteMessageId::Pop3 { .. }) | None => None,
    })
}
