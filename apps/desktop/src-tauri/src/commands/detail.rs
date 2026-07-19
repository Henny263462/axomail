//! The `get_message_detail` command: load, parse, and sanitize one message.
//!
//! The raw RFC 5322 bytes live outside the database under
//! `<raw_root>/raw/<message_id>.eml`. When they are not cached yet and the
//! message has a remote identity, a provider is built to fetch them and the
//! bytes are written back atomically (temp file + rename). Parsing is done by
//! `axomail-parser` and any HTML body is sanitized by `axomail-security`
//! before it is placed in the DTO (ADR-0007).

use std::path::Path;

use axomail_core::{
    Account, IncomingMailProvider, MailError, Mailbox, MailboxId, MessageId, RemoteMessageId,
    SyncRequest,
};
use axomail_parser::parse_message;
use axomail_security::RemoteContentPolicy;
use axomail_storage::{Store, read_raw, write_raw_atomically};
use axomail_sync::ProviderFactory;

// The raw-cache path helper now lives in the storage crate's raw store, shared
// with the sync-time offline download so the `<raw_root>/raw/<id>.eml` layout
// has a single source of truth. Re-exported here so existing callers
// (`account_secrets`, the account-cleanup tests) keep resolving it under
// `commands::detail::raw_message_path`.
pub use axomail_storage::raw_message_path;

use crate::commands::map_command_error;
use crate::dto::MessageDetailDto;
use crate::state::AppState;

/// Batch size used by the selection sync that precedes an on-demand fetch.
const FETCH_SELECT_BATCH: u32 = 50;

/// Loads the full detail of a message, fetching and caching its body if
/// necessary.
///
/// Remote images are blocked unless `allow_remote` overrides it (allow-once)
/// or the message's sender is on the persistent allow list. An explicit
/// `allow_remote` value always wins over the stored per-sender decision; when
/// it is `None`, the stored decision applies.
///
/// When `offline` is set, an on-demand body fetch is never attempted: a
/// message with a remote source that is not already cached resolves to an
/// honest "unavailable offline" placeholder (header from the stored envelope,
/// no body) rather than opening a network connection.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the message is unknown or has no
/// content source (a local draft/outbox message with no cached body), and
/// propagates parse, network, filesystem, or database errors otherwise.
pub async fn get_message_detail_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    message_id: i64,
    allow_remote: Option<bool>,
    offline: bool,
) -> Result<MessageDetailDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let id = MessageId::new(message_id);
    let envelope = store
        .messages()
        .get(id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "the requested message does not exist".to_owned(),
        })?;

    let path = raw_message_path(raw_root, message_id);
    // `freshly_fetched` distinguishes an on-demand download (which must flip
    // `has_raw` and index the body) from reading an already-cached body.
    let (bytes, freshly_fetched) = match read_raw(&path).await? {
        Some(bytes) => (bytes, false),
        None => {
            // Work Offline: do not open a network connection for a body fetch.
            // A remote-backed message that is not cached is simply unavailable
            // offline; return a placeholder instead of hanging or silently
            // syncing. Local-only messages fall through to the existing
            // "no content source" error below.
            if offline && envelope.remote_id.is_some() {
                return MessageDetailDto::offline_placeholder(message_id, &envelope);
            }
            let bytes =
                fetch_and_cache(store, factory, &envelope.remote_id, envelope.mailbox, &path)
                    .await?;
            (bytes, true)
        }
    };

    let parsed = parse_message(&bytes)?;

    // Feed the full body text into the search index; until a message is
    // opened (and thus fetched) only its header/preview text is searchable.
    // When the body was just fetched on demand, flip `has_raw` and index the
    // body in one storage call (`mark_downloaded`) so the offline-cache
    // accounting — cleanup and the download plan — sees the cached file;
    // otherwise just (re)index. Neither failure must break reading the message.
    if freshly_fetched {
        if let Err(error) = store
            .offline()
            .mark_downloaded(id, parsed.text_body.as_deref().unwrap_or_default())
            .await
        {
            tracing::warn!(%message_id, %error, "failed to mark fetched message downloaded");
        }
    } else if let Some(text) = parsed.text_body.as_deref()
        && let Err(error) = store.search().index_message_body(id, text).await
    {
        tracing::warn!(%message_id, %error, "failed to index message body for search");
    }

    // Lazily backfill the threading identity (Message-ID / In-Reply-To /
    // References) the first time a message is opened, for rows synced by an
    // envelope-only path (IMAP) that could not capture them. Idempotent via
    // the repo's `references_json IS NULL` guard; a failure must not break
    // reading the message.
    if let Err(error) = store
        .messages()
        .backfill_threading(id, &parsed.threading_headers())
        .await
    {
        tracing::warn!(%message_id, %error, "failed to backfill threading identity");
    }

    let policy = resolve_remote_policy(store, &parsed, allow_remote).await?;
    MessageDetailDto::from_parsed(message_id, &parsed, policy, envelope.flags.flagged)
}

/// Decides the remote-content policy for this render: an explicit
/// `allow_remote` override wins; otherwise the sender's persisted allow-list
/// entry decides, defaulting to blocking.
async fn resolve_remote_policy(
    store: &Store,
    parsed: &axomail_parser::ParsedMessage,
    allow_remote: Option<bool>,
) -> Result<RemoteContentPolicy, MailError> {
    let allow_remote_images = match allow_remote {
        Some(explicit) => explicit,
        None => match parsed.from.first() {
            Some(sender) => {
                store
                    .remote_content()
                    .is_allowed(sender.email.as_str())
                    .await?
            }
            None => false,
        },
    };
    Ok(RemoteContentPolicy {
        allow_remote_images,
    })
}

/// Fetches the raw body for a message that is not cached yet, caches it, and
/// returns the bytes. A message with no remote identity has no source.
async fn fetch_and_cache<P, F>(
    store: &Store,
    factory: &F,
    remote_id: &Option<RemoteMessageId>,
    mailbox_id: MailboxId,
    path: &Path,
) -> Result<Vec<u8>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let remote_id = remote_id.as_ref().ok_or_else(|| MailError::Configuration {
        message: "this message has no cached body and no remote source to fetch it from".to_owned(),
    })?;
    let mailbox =
        store
            .mailboxes()
            .get(mailbox_id)
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

    let bytes = fetch_raw(store, factory, &account, &mailbox, remote_id).await?;
    write_raw_atomically(path, &bytes).await?;
    Ok(bytes)
}

/// Builds a provider, selects the mailbox via one sync pass (the provider
/// exposes no lighter selection primitive), fetches the body, and closes.
async fn fetch_raw<P, F>(
    store: &Store,
    factory: &F,
    account: &Account,
    mailbox: &Mailbox,
    remote_id: &RemoteMessageId,
) -> Result<Vec<u8>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut provider = factory.make(account).await?;
    provider.connect().await?;
    let state = store.sync_state().get(mailbox.id).await?;
    // A sync pass is the only public way to select the mailbox so a
    // subsequent UID fetch can resolve; its outcome is not ingested here.
    let request = SyncRequest {
        mailbox: mailbox.id,
        remote_name: mailbox.name.clone(),
        state,
        batch_size: FETCH_SELECT_BATCH,
        full_reconcile: false,
    };
    let fetch = async {
        provider.sync(request).await?;
        provider.fetch_message(remote_id).await
    };
    let result = fetch.await;
    let _ = provider.disconnect().await;
    Ok(result?.bytes)
}

/// Loads the full detail of a message for the reading pane.
///
/// `allow_remote` is the allow-once override: `Some(true)` shows remote images
/// for this load regardless of the stored per-sender decision; `None` defers
/// to that decision.
#[tauri::command]
pub async fn get_message_detail(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    allow_remote: Option<bool>,
) -> Result<MessageDetailDto, String> {
    get_message_detail_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        message_id,
        allow_remote,
        state.is_offline(),
    )
    .await
    .map_err(map_command_error)
}
