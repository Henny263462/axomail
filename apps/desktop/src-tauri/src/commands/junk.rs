//! Junk commands: mark-junk / not-junk (train the classifier, set/clear the
//! `$Junk` keyword, move to Junk / back to the Inbox), run junk controls over a
//! folder, and the junk-settings get/set.
//!
//! Marking trains and dispositions through the shared
//! [`axomail_sync::route_to_junk`] / [`route_from_junk`] helpers (local move +
//! keyword + write-back ops), exactly as the sync worker's auto-router does, so
//! a manual and an automatic junking behave identically. Every server op is
//! followed by one `OpReplay` per affected account.

use std::collections::BTreeSet;
use std::path::Path;

use axomail_core::junk::{self, message_tokens};
use axomail_core::{
    AccountId, IncomingMailProvider, MailError, MailboxId, MessageEnvelope, MessageId,
};
use axomail_storage::{MessageCursor, Store};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest, route_from_junk, route_to_junk};

use crate::commands::map_command_error;
use crate::dto::{JunkSettingsDto, MessageMoveDto, MoveOutcomeDto};
use crate::state::AppState;

/// Page size when scanning a folder for the "run junk controls" sweep.
const SCAN_PAGE: u32 = 500;

/// The scoring token set of a stored message: subject, sender, and preview
/// (the best body proxy the envelope carries). Training and scoring share it.
fn tokens_for(envelope: &MessageEnvelope) -> BTreeSet<String> {
    message_tokens(
        envelope.subject.as_deref(),
        &envelope.from,
        envelope.preview.as_deref().unwrap_or_default(),
    )
}

/// The owning account of a message's mailbox, or `None` if either is gone.
async fn message_account(
    store: &Store,
    message: MessageId,
) -> Result<Option<AccountId>, MailError> {
    let Some(envelope) = store.messages().get(message).await? else {
        return Ok(None);
    };
    Ok(store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .map(|mailbox| mailbox.account))
}

/// Triggers one `OpReplay` per affected account (de-duplicated by the engine).
async fn trigger_replays<P, F>(
    engine: &SyncEngine<P, F>,
    accounts: BTreeSet<AccountId>,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    for account_id in accounts {
        engine
            .enqueue_work(WorkRequest::OpReplay { account_id })
            .await?;
    }
    Ok(())
}

/// Records a move for the caller's undo/list-splice when a message actually
/// changed mailbox (a Junk/Inbox folder existed to move into).
async fn moved_record(
    store: &Store,
    message: MessageId,
    source: MailboxId,
) -> Result<Option<MessageMoveDto>, MailError> {
    let Some(envelope) = store.messages().get(message).await? else {
        return Ok(None);
    };
    if envelope.mailbox == source {
        return Ok(None);
    }
    Ok(Some(MessageMoveDto::new(message.value(), source.value())?))
}

/// Marks a selection as junk: trains each message as spam, files it to Junk,
/// and propagates. Returns the per-message moves so the list can splice them.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn mark_junk_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut moves = Vec::new();
    let mut accounts = BTreeSet::new();
    for id in message_ids {
        let message = MessageId::new(id);
        let Some(envelope) = store.messages().get(message).await? else {
            continue;
        };
        let Some(account) = message_account(store, message).await? else {
            continue;
        };
        let source = envelope.mailbox;
        store
            .junk()
            .train(account, &tokens_for(&envelope), true)
            .await?;
        // Pin the score high so a later ingest pass never re-evaluates it as ham.
        store.junk().set_score(message, 1.0).await?;
        if let Some(op_account) = route_to_junk(store, message).await? {
            accounts.insert(op_account);
        }
        if let Some(record) = moved_record(store, message, source).await? {
            moves.push(record);
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(MoveOutcomeDto { moves })
}

/// Marks a selection as not junk: trains each message as ham, clears `$Junk`,
/// moves it back to the Inbox, and propagates.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn not_junk_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut moves = Vec::new();
    let mut accounts = BTreeSet::new();
    for id in message_ids {
        let message = MessageId::new(id);
        let Some(envelope) = store.messages().get(message).await? else {
            continue;
        };
        let Some(account) = message_account(store, message).await? else {
            continue;
        };
        let source = envelope.mailbox;
        store
            .junk()
            .train(account, &tokens_for(&envelope), false)
            .await?;
        // Pin the score low so it is never auto-junked again.
        store.junk().set_score(message, 0.0).await?;
        if let Some(op_account) = route_from_junk(store, message).await? {
            accounts.insert(op_account);
        }
        if let Some(record) = moved_record(store, message, source).await? {
            moves.push(record);
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(MoveOutcomeDto { moves })
}

/// Re-scores every message in `mailbox` and files those over the threshold as
/// junk (respecting the whitelist and, when enabled and the raw is cached, a
/// server `X-Spam-*` verdict). Returns how many were newly filed.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn run_junk_controls_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    raw_root: &Path,
    mailbox_id: i64,
) -> Result<u32, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox = MailboxId::new(mailbox_id);
    let Some(account) = store
        .mailboxes()
        .get(mailbox)
        .await?
        .map(|mailbox| mailbox.account)
    else {
        return Ok(0);
    };
    let settings = store.junk().settings().await?;
    let envelopes = collect_folder(store, mailbox).await?;

    let mut accounts = BTreeSet::new();
    let mut filed = 0u32;
    for envelope in envelopes {
        let message = envelope.id;
        let score =
            match server_verdict(raw_root, message.value(), settings.trust_server_headers).await? {
                Some(true) => 1.0,
                Some(false) => 0.0,
                None => store.junk().score(account, &tokens_for(&envelope)).await?,
            };
        store.junk().set_score(message, score).await?;
        if score < settings.threshold || whitelisted(store, &envelope, &settings).await? {
            continue;
        }
        let source = envelope.mailbox;
        if let Some(op_account) = route_to_junk(store, message).await? {
            accounts.insert(op_account);
        }
        if moved_record(store, message, source).await?.is_some() {
            filed += 1;
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(filed)
}

/// Whether the sender is a whitelisted contact (only when the setting is on).
async fn whitelisted(
    store: &Store,
    envelope: &MessageEnvelope,
    settings: &axomail_storage::JunkSettings,
) -> Result<bool, MailError> {
    if !settings.whitelist_contacts {
        return Ok(false);
    }
    let Some(sender) = envelope.from.first() else {
        return Ok(false);
    };
    store
        .junk()
        .is_whitelisted_sender(sender.email.as_str())
        .await
}

/// Reads a cached message's server spam verdict, or `None` when trust is off or
/// no raw is cached.
async fn server_verdict(
    raw_root: &Path,
    message_id: i64,
    trust: bool,
) -> Result<Option<bool>, MailError> {
    if !trust {
        return Ok(None);
    }
    let path = axomail_storage::raw_message_path(raw_root, message_id);
    let Some(bytes) = axomail_storage::read_raw(&path).await? else {
        return Ok(None);
    };
    Ok(junk::server_spam_verdict(&String::from_utf8_lossy(&bytes)))
}

/// Pages through a mailbox, snapshotting its envelopes before any routing
/// mutates the folder.
async fn collect_folder(
    store: &Store,
    mailbox: MailboxId,
) -> Result<Vec<MessageEnvelope>, MailError> {
    let mut cursor: Option<MessageCursor> = None;
    let mut all = Vec::new();
    loop {
        let page = store
            .messages()
            .list_page(mailbox, cursor, SCAN_PAGE)
            .await?;
        all.extend(page.items);
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(all)
}

/// Reads the current junk settings.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn get_junk_settings_impl(store: &Store) -> Result<JunkSettingsDto, MailError> {
    Ok(JunkSettingsDto::from_settings(
        store.junk().settings().await?,
    ))
}

/// Persists the junk settings.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_junk_settings_impl(
    store: &Store,
    settings: JunkSettingsDto,
) -> Result<(), MailError> {
    store.junk().set_settings(&settings.into_settings()).await
}

/// Marks a selection of messages as junk.
#[tauri::command]
pub async fn mark_junk(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, String> {
    mark_junk_impl(state.store(), state.engine(), message_ids)
        .await
        .map_err(map_command_error)
}

/// Marks a selection of messages as not junk.
#[tauri::command]
pub async fn not_junk(
    state: tauri::State<'_, AppState>,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, String> {
    not_junk_impl(state.store(), state.engine(), message_ids)
        .await
        .map_err(map_command_error)
}

/// Runs junk controls over a folder, filing over-threshold mail as junk.
#[tauri::command]
pub async fn run_junk_controls(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
) -> Result<u32, String> {
    run_junk_controls_impl(state.store(), state.engine(), state.raw_root(), mailbox_id)
        .await
        .map_err(map_command_error)
}

/// Reads the junk settings.
#[tauri::command]
pub async fn get_junk_settings(
    state: tauri::State<'_, AppState>,
) -> Result<JunkSettingsDto, String> {
    get_junk_settings_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Persists the junk settings.
#[tauri::command]
pub async fn set_junk_settings(
    state: tauri::State<'_, AppState>,
    settings: JunkSettingsDto,
) -> Result<(), String> {
    set_junk_settings_impl(state.store(), settings)
        .await
        .map_err(map_command_error)
}
