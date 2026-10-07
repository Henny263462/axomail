//! Reply / reply-all / forward: composer prefill and post-send source marking.
//!
//! [`prepare_reply`] loads a source message (fetching and caching its raw body
//! if needed), then builds a [`ComposerPrefillDto`] — recipients, `Re:`/`Fwd:`
//! subject, quoted/forwarded body arranged per the identity's composition
//! preferences, threading headers, and (for a forward-as-attachment) the
//! original as a `message/rfc822` part. The composer opens pre-filled from it.
//!
//! After the composed message is sent, the composer calls [`mark_reply_source`]
//! to set `\Answered` (a reply) or add the `$Forwarded` keyword (a forward) on
//! the original, applied locally and propagated to the server via the Wave 2
//! op queue — exactly the write-back path the flag commands use.
//!
//! The reusable, store-free pieces live in the submodules: [`build`] assembles
//! the DTO, [`quote`] the citation/body, [`recipients`] the address derivation,
//! and [`subject`] the prefix normalization.

mod build;
mod quote;
mod recipients;
mod subject;

use std::path::Path;

use axomail_core::{
    AccountId, IncomingMailProvider, IncomingProtocol, Keyword, MailError, MessageEnvelope,
    MessageId,
};
use axomail_parser::parse_message;
use axomail_storage::{ForwardMode, NewPendingOp, Store, raw_message_path, read_raw};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, answered_op, keyword_op};

use self::build::{build_prefill, guess_identity, self_email_set};
use crate::commands::detail::fetch_and_cache;
use crate::commands::map_command_error;
use crate::dto::{ComposerPrefillDto, ReplyMode, SourceMark};
use crate::state::AppState;

/// The IMAP keyword marking a message as forwarded (Thunderbird-compatible).
const FORWARDED_KEYWORD: &str = "$Forwarded";

/// The reply operation being prepared, mapped from the [`ReplyMode`] DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Reply to the sender only.
    Reply,
    /// Reply to the sender and every other recipient.
    ReplyAll,
    /// Forward to new recipients; `Some` overrides the identity's forward mode.
    Forward(Option<ForwardMode>),
}

impl From<ReplyMode> for Mode {
    fn from(mode: ReplyMode) -> Self {
        match mode {
            ReplyMode::Reply => Self::Reply,
            ReplyMode::ReplyAll => Self::ReplyAll,
            ReplyMode::Forward => Self::Forward(None),
            ReplyMode::ForwardInline => Self::Forward(Some(ForwardMode::Inline)),
            ReplyMode::ForwardAttachment => Self::Forward(Some(ForwardMode::Attachment)),
        }
    }
}

/// Builds the composer prefill for a reply / reply-all / forward.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the message (or its mailbox or
/// account) does not exist, when it has no cached body and no remote source, or
/// when it is not cached and the app is working offline. Propagates parse,
/// network, filesystem, and database errors otherwise.
pub async fn prepare_reply_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    message_id: i64,
    mode: ReplyMode,
    offline: bool,
) -> Result<ComposerPrefillDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let envelope = store
        .messages()
        .get(MessageId::new(message_id))
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message to reply to no longer exists".to_owned(),
        })?;
    let mailbox = store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message's mailbox no longer exists".to_owned(),
        })?;
    let account = store
        .accounts()
        .get(mailbox.account)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the message's account no longer exists".to_owned(),
        })?;

    let bytes = load_source_bytes(store, factory, raw_root, &envelope, offline).await?;
    let parsed = parse_message(&bytes)?;

    let identities = store.identities().list(account.id).await?;
    let self_emails = self_email_set(account.email.as_str(), &identities);
    let identity = guess_identity(&parsed, &identities);

    build_prefill(
        &parsed,
        &bytes,
        Mode::from(mode),
        account.id.value(),
        message_id,
        identity,
        &self_emails,
    )
    .checked()
}

/// Loads the source `.eml`: from the raw cache, or by fetching and caching it
/// on demand. A message with no cached body and no remote source, or one that
/// is not cached while offline, cannot be replied to.
async fn load_source_bytes<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    envelope: &MessageEnvelope,
    offline: bool,
) -> Result<Vec<u8>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let path = raw_message_path(raw_root, envelope.id.value());
    if let Some(bytes) = read_raw(&path).await? {
        return Ok(bytes);
    }
    if envelope.remote_id.is_none() {
        return Err(MailError::Configuration {
            message: "this message has no cached body to reply to".to_owned(),
        });
    }
    if offline {
        return Err(MailError::Configuration {
            message: "this message is not available offline; open it while online first".to_owned(),
        });
    }
    fetch_and_cache(store, factory, &envelope.remote_id, envelope.mailbox, &path).await
}

/// Marks a source message answered or forwarded after its reply/forward was
/// sent, applying locally and enqueuing a write-back op for IMAP accounts.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn mark_reply_source_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_id: i64,
    mark: SourceMark,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    if let Some(account_id) = apply_source_mark(store, MessageId::new(message_id), mark).await? {
        engine
            .enqueue_work(WorkRequest::OpReplay { account_id })
            .await?;
    }
    Ok(())
}

/// Applies the source mark locally and, when propagation applies (IMAP account,
/// message has a server identity), enqueues a write-back op. Returns the account
/// whose queue gained an op.
async fn apply_source_mark(
    store: &Store,
    message_id: MessageId,
    mark: SourceMark,
) -> Result<Option<AccountId>, MailError> {
    let Some(envelope) = store.messages().get(message_id).await? else {
        return Ok(None);
    };

    // `\Answered` has a local column, so record it optimistically (idempotent).
    // `$Forwarded` is an IMAP keyword with no local column here, so it is
    // server-propagation only — never applied to local state.
    if mark == SourceMark::Answered && !envelope.flags.answered {
        let mut flags = envelope.flags.clone();
        flags.answered = true;
        store.messages().update_flags(message_id, &flags).await?;
    }

    // Server propagation needs a server identity and an IMAP account.
    if envelope.remote_id.is_none() {
        return Ok(None);
    }
    let Some(mailbox) = store.mailboxes().get(envelope.mailbox).await? else {
        return Ok(None);
    };
    let Some(account) = store.accounts().get(mailbox.account).await? else {
        return Ok(None);
    };
    if account.incoming.protocol != IncomingProtocol::Imap {
        return Ok(None);
    }

    let (kind, payload_json) = match mark {
        SourceMark::Answered => answered_op(true),
        SourceMark::Forwarded => keyword_op(Keyword::parse(FORWARDED_KEYWORD)?, true),
    };
    store
        .pending_ops()
        .enqueue(&NewPendingOp {
            account_id: account.id,
            message_id,
            kind,
            payload_json,
        })
        .await?;
    Ok(Some(account.id))
}

/// Builds the composer prefill for a reply, reply-all, or forward.
#[tauri::command]
pub async fn prepare_reply(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    mode: ReplyMode,
) -> Result<ComposerPrefillDto, String> {
    prepare_reply_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        message_id,
        mode,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
}

/// Marks a source message answered (`\Answered`) or forwarded (`$Forwarded`)
/// after its reply/forward has been sent.
#[tauri::command]
pub async fn mark_reply_source(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    mark: SourceMark,
) -> Result<(), String> {
    mark_reply_source_impl(state.store(), state.engine(), message_id, mark)
        .await
        .map_err(map_command_error)
}
