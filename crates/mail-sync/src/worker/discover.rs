//! The `Discover` executor: list the account's remote folder tree and
//! reconcile it into local storage.
//!
//! Mirrors the durable connection shape of [`mailbox_sync`](super::mailbox_sync)
//! — build a provider, connect, run the op, disconnect (best effort) — with
//! every network round-trip wrapped in [`super::guarded`] for uniform timeout
//! and cancellation. The reconcile itself
//! ([`MailboxRepo::reconcile`](axomail_storage::MailboxRepo::reconcile)) is a
//! single local transaction that never drops a folder holding local messages.

use std::sync::Arc;

use axomail_core::{
    AccountId, IncomingMailProvider, MailError, MailboxRole, RemoteMailbox, RemoteMailboxState,
};
use axomail_storage::DiscoveredMailbox;
use tokio_util::sync::CancellationToken;

use crate::event::SyncJobSummary;
use crate::factory::ProviderFactory;
use crate::shared::Shared;

use super::guarded;

/// Discovers and reconciles the account's folder tree.
pub(super) async fn execute<P, F>(
    shared: &Arc<Shared<P, F>>,
    account_id: AccountId,
    token: &CancellationToken,
) -> Result<SyncJobSummary, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let account = shared
        .store
        .accounts()
        .get(account_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "account for discover job no longer exists".to_owned(),
        })?;

    let net = shared.config.network_timeout;
    let mut provider = guarded(token, net, shared.factory.make(&account)).await?;
    let discovered = run(shared, &mut provider, token).await;
    // Close the session regardless of how discovery ended.
    let _ = guarded(token, net, provider.disconnect()).await;
    let state = discovered?;

    let folders: Vec<DiscoveredMailbox> = state.mailboxes.iter().map(to_discovered).collect();
    shared
        .store
        .mailboxes()
        .reconcile(account_id, &folders)
        .await?;

    // Folder discovery ingests no messages; report an empty summary so the UI
    // never renders it as new mail.
    Ok(SyncJobSummary::default())
}

/// Connects and lists the remote folders.
async fn run<P, F>(
    shared: &Arc<Shared<P, F>>,
    provider: &mut P,
    token: &CancellationToken,
) -> Result<RemoteMailboxState, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let net = shared.config.network_timeout;
    guarded(token, net, provider.connect()).await?;
    guarded(token, net, provider.discover()).await
}

/// Maps a server-reported folder to a storage [`DiscoveredMailbox`], resolving
/// its role from the SPECIAL-USE hint, else a leaf-name guess.
fn to_discovered(remote: &RemoteMailbox) -> DiscoveredMailbox {
    let role = remote
        .role
        .unwrap_or_else(|| MailboxRole::guess_from_name(leaf(remote)));
    DiscoveredMailbox {
        remote_name: remote.name.clone(),
        role,
        delimiter: remote.delimiter.clone(),
        // LIST reports the folder set to display; treat every discovered folder
        // as subscribed until the user unsubscribes it locally.
        subscribed: true,
    }
}

/// The last path segment of a folder, split on its delimiter (falls back to the
/// whole name when the server reported no delimiter).
fn leaf(remote: &RemoteMailbox) -> &str {
    match remote.delimiter.as_deref() {
        Some(delimiter) if !delimiter.is_empty() => remote
            .name
            .as_str()
            .rsplit(delimiter)
            .next()
            .unwrap_or_else(|| remote.name.as_str()),
        _ => remote.name.as_str(),
    }
}
