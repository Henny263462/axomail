//! Folder-management commands: create, rename, delete, (un)subscribe, and a
//! manual "refresh folders" that enqueues a discovery job.
//!
//! The CRUD commands are **server-first**: the server operation runs first, and
//! local storage is touched only after it succeeds. A server failure therefore
//! surfaces as a clean error and never leaves a phantom folder behind. Discovery
//! is asynchronous — `refresh_folders` enqueues a [`WorkRequest::Discover`] job
//! and returns immediately; the reconcile runs on the sync engine.

use axomail_core::{
    Account, AccountId, IncomingMailProvider, IncomingProtocol, MailError, Mailbox, MailboxId,
    MailboxName, MailboxRole,
};
use axomail_storage::{MailboxCounts, Store};
use axomail_sync::{ProviderFactory, SyncEngine, WorkRequest};

use crate::commands::map_command_error;
use crate::dto::{
    CreateFolderInput, MailboxSummaryDto, RenameFolderInput, SetFolderSubscribedInput,
};
use crate::state::AppState;

/// Enqueues a folder-discovery job for an account only if it is an IMAP account
/// (POP3 keeps its local seed). Best-effort: a lookup or enqueue failure is
/// logged, never surfaced, since the triggering action (account creation) has
/// already succeeded. Shared by account creation and the refresh action.
pub(crate) async fn enqueue_discover_if_imap(state: &AppState, account_id: i64) {
    let id = AccountId::new(account_id);
    let protocol = match state.store().accounts().get(id).await {
        Ok(Some(account)) => account.incoming.protocol,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(error = %error, "could not read account for folder discovery");
            return;
        }
    };
    if protocol != IncomingProtocol::Imap {
        return;
    }
    if let Err(error) = state
        .engine()
        .enqueue_work(WorkRequest::Discover { account_id: id })
        .await
    {
        tracing::warn!(error = %error, "could not enqueue folder discovery");
    }
}

/// Creates a folder on the server, then records it locally.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown account/parent, a blank
/// or delimiter-bearing name, or a name that already exists, and propagates any
/// network/protocol error from the server `CREATE`.
pub async fn create_folder_impl<P, F>(
    store: &Store,
    factory: &F,
    input: CreateFolderInput,
) -> Result<MailboxSummaryDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account_id = AccountId::new(input.account_id);
    let account = store
        .accounts()
        .get(account_id)
        .await?
        .ok_or_else(|| config("the account does not exist"))?;
    let mailboxes = store.mailboxes().list(account_id).await?;

    let leaf = input.name.trim();
    if leaf.is_empty() {
        return Err(config("the folder name must not be empty"));
    }

    let (parent_id, remote_name, delimiter) = resolve_new_path(&mailboxes, input.parent_id, leaf)?;
    if mailboxes
        .iter()
        .any(|mailbox| mailbox.remote_name.as_str() == remote_name)
    {
        return Err(config("a folder with that name already exists"));
    }

    let remote = MailboxName::new(remote_name);
    let mut provider = connect(factory, &account).await?;
    let outcome = provider.create_mailbox(&remote).await;
    let _ = provider.disconnect().await;
    outcome?;

    let mailbox = store
        .mailboxes()
        .create_local(
            account_id,
            parent_id,
            &remote,
            delimiter.as_deref(),
            MailboxRole::Custom,
        )
        .await?;
    MailboxSummaryDto::from_domain(&mailbox, MailboxCounts::default())
}

/// Renames a folder (and its subtree) on the server, then locally.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder, a blank or
/// delimiter-bearing name, or a name collision, and propagates the server
/// `RENAME` error.
pub async fn rename_folder_impl<P, F>(
    store: &Store,
    factory: &F,
    input: RenameFolderInput,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let (account, folder) = load_folder(store, input.folder_id).await?;
    let leaf = input.name.trim();
    if leaf.is_empty() {
        return Err(config("the folder name must not be empty"));
    }
    if let Some(delimiter) = folder.delimiter.as_deref()
        && leaf.contains(delimiter)
    {
        return Err(config(
            "the folder name must not contain the folder delimiter",
        ));
    }

    let new_remote = match folder.parent_remote_name() {
        Some(parent) => {
            let delimiter = folder
                .delimiter
                .as_deref()
                .ok_or_else(|| config("the folder is missing its delimiter; refresh folders"))?;
            format!("{parent}{delimiter}{leaf}")
        }
        None => leaf.to_owned(),
    };
    if new_remote == folder.remote_name.as_str() {
        return Ok(());
    }

    let mailboxes = store.mailboxes().list(account.id).await?;
    if mailboxes
        .iter()
        .any(|mailbox| mailbox.id != folder.id && mailbox.remote_name.as_str() == new_remote)
    {
        return Err(config("a folder with that name already exists"));
    }

    let new_remote = MailboxName::new(new_remote);
    let mut provider = connect(factory, &account).await?;
    let outcome = provider
        .rename_mailbox(&folder.remote_name, &new_remote)
        .await;
    let _ = provider.disconnect().await;
    outcome?;
    store
        .mailboxes()
        .rename_subtree(folder.id, &new_remote)
        .await
}

/// Deletes a folder on the server, then locally.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder and propagates
/// the server `DELETE` error.
pub async fn delete_folder_impl<P, F>(
    store: &Store,
    factory: &F,
    folder_id: i64,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let (account, folder) = load_folder(store, folder_id).await?;
    let mut provider = connect(factory, &account).await?;
    let outcome = provider.delete_mailbox(&folder.remote_name).await;
    let _ = provider.disconnect().await;
    outcome?;
    store.mailboxes().delete(folder.id).await
}

/// Subscribes to or unsubscribes from a folder on the server, then locally.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder and propagates
/// the server `SUBSCRIBE`/`UNSUBSCRIBE` error.
pub async fn set_folder_subscribed_impl<P, F>(
    store: &Store,
    factory: &F,
    input: SetFolderSubscribedInput,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let (account, folder) = load_folder(store, input.folder_id).await?;
    let mut provider = connect(factory, &account).await?;
    let outcome = provider
        .set_mailbox_subscribed(&folder.remote_name, input.subscribed)
        .await;
    let _ = provider.disconnect().await;
    outcome?;
    store
        .mailboxes()
        .set_subscribed(folder.id, input.subscribed)
        .await
}

/// Enqueues a folder-discovery job for an account (the manual "refresh folders"
/// action). Returns as soon as the job is queued; the reconcile runs on the
/// engine. POP3 accounts have no server folders to discover, so this is a
/// no-op for them.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown account and
/// [`MailError::Cancelled`] if the engine has shut down.
pub async fn refresh_folders_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    account_id: i64,
) -> Result<(), MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let id = AccountId::new(account_id);
    let account = store
        .accounts()
        .get(id)
        .await?
        .ok_or_else(|| config("the account does not exist"))?;
    if account.incoming.protocol != IncomingProtocol::Imap {
        return Ok(());
    }
    engine
        .enqueue_work(WorkRequest::Discover { account_id: id })
        .await
}

/// Resolves the parent id, full remote path, and delimiter for a new folder.
fn resolve_new_path(
    mailboxes: &[Mailbox],
    parent_id: Option<i64>,
    leaf: &str,
) -> Result<(Option<MailboxId>, String, Option<String>), MailError> {
    match parent_id {
        Some(pid) => {
            let parent = mailboxes
                .iter()
                .find(|mailbox| mailbox.id.value() == pid)
                .ok_or_else(|| config("the parent folder does not exist"))?;
            let delimiter = parent
                .delimiter
                .clone()
                .or_else(|| account_delimiter(mailboxes))
                .ok_or_else(|| config("cannot determine the folder delimiter; refresh folders"))?;
            if leaf.contains(&delimiter) {
                return Err(config(
                    "the folder name must not contain the folder delimiter",
                ));
            }
            let remote = format!("{}{}{}", parent.remote_name.as_str(), delimiter, leaf);
            Ok((Some(parent.id), remote, Some(delimiter)))
        }
        None => {
            let delimiter = account_delimiter(mailboxes);
            if let Some(delimiter) = &delimiter
                && leaf.contains(delimiter)
            {
                return Err(config(
                    "the folder name must not contain the folder delimiter",
                ));
            }
            Ok((None, leaf.to_owned(), delimiter))
        }
    }
}

/// The account's server delimiter: the first non-empty one among its folders.
fn account_delimiter(mailboxes: &[Mailbox]) -> Option<String> {
    mailboxes
        .iter()
        .find_map(|mailbox| mailbox.delimiter.clone().filter(|d| !d.is_empty()))
}

/// Loads a folder and its owning account by folder id.
async fn load_folder(store: &Store, folder_id: i64) -> Result<(Account, Mailbox), MailError> {
    let folder = store
        .mailboxes()
        .get(MailboxId::new(folder_id))
        .await?
        .ok_or_else(|| config("the folder does not exist"))?;
    let account = store
        .accounts()
        .get(folder.account)
        .await?
        .ok_or_else(|| config("the folder's account no longer exists"))?;
    Ok((account, folder))
}

/// Builds a provider and connects it. The caller runs the folder operation and
/// then disconnects (best effort), propagating the op error after the
/// disconnect so a failed op never leaves the session open.
async fn connect<P, F>(factory: &F, account: &Account) -> Result<P, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut provider = factory.make(account).await?;
    provider.connect().await?;
    Ok(provider)
}

/// Shorthand for a user-facing configuration error.
fn config(message: &str) -> MailError {
    MailError::Configuration {
        message: message.to_owned(),
    }
}

/// Creates a folder and returns its summary.
#[tauri::command]
pub async fn create_folder(
    state: tauri::State<'_, AppState>,
    input: CreateFolderInput,
) -> Result<MailboxSummaryDto, String> {
    create_folder_impl(state.store(), state.factory(), input)
        .await
        .map_err(map_command_error)
}

/// Renames a folder.
#[tauri::command]
pub async fn rename_folder(
    state: tauri::State<'_, AppState>,
    input: RenameFolderInput,
) -> Result<(), String> {
    rename_folder_impl(state.store(), state.factory(), input)
        .await
        .map_err(map_command_error)
}

/// Deletes a folder.
#[tauri::command]
pub async fn delete_folder(
    state: tauri::State<'_, AppState>,
    folder_id: i64,
) -> Result<(), String> {
    delete_folder_impl(state.store(), state.factory(), folder_id)
        .await
        .map_err(map_command_error)
}

/// Subscribes to or unsubscribes from a folder.
#[tauri::command]
pub async fn set_folder_subscribed(
    state: tauri::State<'_, AppState>,
    input: SetFolderSubscribedInput,
) -> Result<(), String> {
    set_folder_subscribed_impl(state.store(), state.factory(), input)
        .await
        .map_err(map_command_error)
}

/// Enqueues a folder-discovery refresh for an account.
#[tauri::command]
pub async fn refresh_folders(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<(), String> {
    refresh_folders_impl(state.store(), state.engine(), account_id)
        .await
        .map_err(map_command_error)
}
