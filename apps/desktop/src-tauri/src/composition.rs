//! Composition root for the incoming-provider stack (ADR-0004).
//!
//! `mail-sync` is generic over an [`IncomingMailProvider`] and a
//! [`ProviderFactory`]; the concrete protocol crates are named only here, in
//! the application layer. [`AnyIncomingProvider`] is the dispatch enum the
//! ADR prescribes (IMAP and POP3 arms), and [`AppProviderFactory`] builds one
//! for an account by resolving its incoming secret from the OS keychain and
//! constructing the matching adapter with production TLS trust (the
//! Mozilla/webpki bundle only; no extra roots).
//!
//! The POP3 arm is wrapped in [`Pop3Sync`], which threads the local store so
//! it can fill the adapter's known-`UIDL` set from the `pop3_uidl` table
//! before each sync and record newly ingested `UIDL`s afterwards.

use std::sync::Arc;

use axomail_core::{
    Account, AccountAuth, ConnectionCredential, CredentialRef, FlagDelta, IncomingMailProvider,
    IncomingProtocol, MailError, MailboxName, RawMessage, RemoteMailboxState, RemoteMessageId,
    SyncOutcome, SyncRequest,
};
use axomail_imap::{ImapConfig, ImapProvider};
use axomail_pop3::{Pop3Config, Pop3Provider};
use axomail_security::credential_ref_for_oauth;
use axomail_storage::Store;
use axomail_sync::ProviderFactory;

use crate::pop3_sync::Pop3Sync;
use crate::settings;
use crate::state::SharedSecretStore;

/// Enum-dispatched incoming provider (ADR-0004: no `dyn`, no `async-trait`).
///
/// Each trait method delegates to the wrapped adapter by `match`. The POP3
/// arm carries a [`Pop3Sync`] rather than a bare adapter so the known-`UIDL`
/// bookkeeping happens transparently on every sync.
pub enum AnyIncomingProvider {
    /// The IMAP adapter.
    Imap(ImapProvider),
    /// The POP3 adapter with its storage-backed de-duplication.
    Pop3(Pop3Sync),
}

impl IncomingMailProvider for AnyIncomingProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.connect().await,
            Self::Pop3(provider) => provider.connect().await,
        }
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        match self {
            Self::Imap(provider) => provider.discover().await,
            Self::Pop3(provider) => provider.discover().await,
        }
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        match self {
            Self::Imap(provider) => provider.sync(request).await,
            Self::Pop3(provider) => provider.sync(request).await,
        }
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        match self {
            Self::Imap(provider) => provider.fetch_message(id).await,
            Self::Pop3(provider) => provider.fetch_message(id).await,
        }
    }

    async fn select_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.select_mailbox(mailbox).await,
            Self::Pop3(provider) => provider.select_mailbox(mailbox).await,
        }
    }

    async fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.store_flags(id, delta).await,
            Self::Pop3(provider) => provider.store_flags(id, delta).await,
        }
    }

    async fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.move_message(id, target).await,
            Self::Pop3(provider) => provider.move_message(id, target).await,
        }
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.delete_message(id).await,
            Self::Pop3(provider) => provider.delete_message(id).await,
        }
    }

    async fn create_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.create_mailbox(path).await,
            Self::Pop3(provider) => provider.create_mailbox(path).await,
        }
    }

    async fn rename_mailbox(
        &mut self,
        from: &MailboxName,
        to: &MailboxName,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.rename_mailbox(from, to).await,
            Self::Pop3(provider) => provider.rename_mailbox(from, to).await,
        }
    }

    async fn delete_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.delete_mailbox(path).await,
            Self::Pop3(provider) => provider.delete_mailbox(path).await,
        }
    }

    async fn set_mailbox_subscribed(
        &mut self,
        path: &MailboxName,
        subscribed: bool,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.set_mailbox_subscribed(path, subscribed).await,
            Self::Pop3(provider) => provider.set_mailbox_subscribed(path, subscribed).await,
        }
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.disconnect().await,
            Self::Pop3(provider) => provider.disconnect().await,
        }
    }
}

/// Builds an [`AnyIncomingProvider`] for an account (the [`ProviderFactory`]
/// the sync engine drives).
///
/// Holds the local [`Store`] as well as the secret store: the POP3 arm reads
/// its per-account delete policy from settings and needs the store for its
/// `UIDL` bookkeeping.
#[derive(Clone)]
pub struct AppProviderFactory {
    secrets: SharedSecretStore,
    store: Store,
}

impl AppProviderFactory {
    /// Creates a factory that resolves credentials through `secrets` and
    /// reads settings and `UIDL` state through `store`.
    #[must_use]
    pub fn new(secrets: SharedSecretStore, store: Store) -> Self {
        Self { secrets, store }
    }
}

impl ProviderFactory<AnyIncomingProvider> for AppProviderFactory {
    async fn make(&self, account: &Account) -> Result<AnyIncomingProvider, MailError> {
        let credential =
            resolve_credential(&self.secrets, account, &account.incoming.credential).await?;
        match account.incoming.protocol {
            IncomingProtocol::Imap => {
                // Production trust: the webpki bundle only. `extra_root_certs_der`
                // stays empty and there is no code path that weakens verification.
                let config = ImapConfig::with_credential(
                    account.incoming.endpoint.clone(),
                    account.incoming.security,
                    account.incoming.username.clone(),
                    credential,
                );
                Ok(AnyIncomingProvider::Imap(ImapProvider::new(config)))
            }
            IncomingProtocol::Pop3 => {
                // The per-account policy is the only switch that lifts POP3's
                // delete block; account setup never sets it.
                let delete_after_download =
                    settings::pop3_delete_enabled(&self.store, account.id.value()).await?;
                let mut config = Pop3Config::with_credential(
                    account.incoming.endpoint.clone(),
                    account.incoming.security,
                    account.incoming.username.clone(),
                    credential,
                );
                config.delete_after_download = delete_after_download;
                let provider = Pop3Provider::new(config);
                Ok(AnyIncomingProvider::Pop3(Pop3Sync::new(
                    provider,
                    self.store.clone(),
                    account.id,
                )))
            }
        }
    }
}

/// Resolves the connection credential for one direction of an account.
///
/// Password accounts read their per-direction password from `reference` in the
/// keychain. OAuth2 accounts ignore the per-direction `reference` and resolve
/// the account's **single** shared token set (see
/// [`credential_ref_for_oauth`]), refreshing it when due (rotating the stored
/// refresh token) and yielding a fresh access token for XOAUTH2. The secret is
/// never logged or placed in an error.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if no secret/token is stored (an
/// OAuth2 account that has not signed in resolves to a user-actionable
/// "sign in again"), plus any keychain, network, or authentication error from
/// an OAuth refresh.
pub(crate) async fn resolve_credential(
    secrets: &SharedSecretStore,
    account: &Account,
    reference: &CredentialRef,
) -> Result<ConnectionCredential, MailError> {
    match &account.auth {
        AccountAuth::Password => Ok(ConnectionCredential::password(
            resolve_secret(secrets, reference).await?,
        )),
        AccountAuth::OAuth2(config) => {
            // Both directions share one token set, keyed by the account, so a
            // rotating refresh token can never invalidate the "other"
            // direction's copy.
            let oauth_ref = credential_ref_for_oauth(account.id);
            Ok(ConnectionCredential::xoauth2(
                crate::oauth::resolve_access_token(secrets, &oauth_ref, config).await?,
            ))
        }
    }
}

/// Resolves a keychain credential to its secret on a blocking task.
///
/// Keychain I/O blocks, so it runs on `spawn_blocking`. A missing secret is
/// a configuration problem, not a keychain fault. The secret itself is never
/// logged and never placed in the returned error.
///
/// # Errors
///
/// Returns [`MailError::Keychain`] if the blocking task fails to join or the
/// store cannot be read, and [`MailError::Configuration`] if no secret is
/// stored for the reference.
pub(crate) async fn resolve_secret(
    secrets: &SharedSecretStore,
    reference: &CredentialRef,
) -> Result<String, MailError> {
    let secrets = Arc::clone(secrets);
    let reference = reference.clone();
    let secret = match tokio::task::spawn_blocking(move || secrets.get_secret(&reference)).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(MailError::Keychain {
                message: "keychain read did not complete".to_owned(),
            });
        }
    };
    secret.ok_or_else(|| MailError::Configuration {
        message: "no stored password for this account; re-enter the credentials".to_owned(),
    })
}
