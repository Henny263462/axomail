//! Composition root for the incoming-provider stack (ADR-0004).
//!
//! `mail-sync` is generic over an [`IncomingMailProvider`] and a
//! [`ProviderFactory`]; the concrete protocol crates are named only here, in
//! the application layer. [`AnyIncomingProvider`] is the dispatch enum the
//! ADR prescribes (a POP3 arm joins in phase 3), and [`AppProviderFactory`]
//! builds one for an account by resolving its incoming secret from the OS
//! keychain and constructing an [`ImapProvider`] with production TLS trust
//! (the Mozilla/webpki bundle only; no extra roots).

use std::sync::Arc;

use axomail_core::{
    Account, CredentialRef, IncomingMailProvider, IncomingProtocol, MailError, MailboxName,
    MessageFlags, RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_imap::{ImapConfig, ImapProvider};
use axomail_sync::ProviderFactory;

use crate::state::SharedSecretStore;

/// Enum-dispatched incoming provider (ADR-0004: no `dyn`, no `async-trait`).
///
/// Only the IMAP arm exists today; POP3 (`axomail-pop3`) is added in phase 3.
/// Each trait method delegates to the wrapped adapter by `match`.
pub enum AnyIncomingProvider {
    /// The IMAP adapter.
    Imap(ImapProvider),
}

impl IncomingMailProvider for AnyIncomingProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.connect().await,
        }
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        match self {
            Self::Imap(provider) => provider.discover().await,
        }
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        match self {
            Self::Imap(provider) => provider.sync(request).await,
        }
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        match self {
            Self::Imap(provider) => provider.fetch_message(id).await,
        }
    }

    async fn set_flags(
        &mut self,
        id: &RemoteMessageId,
        flags: MessageFlags,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.set_flags(id, flags).await,
        }
    }

    async fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.move_message(id, target).await,
        }
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.delete_message(id).await,
        }
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        match self {
            Self::Imap(provider) => provider.disconnect().await,
        }
    }
}

/// Builds an [`AnyIncomingProvider`] for an account (the [`ProviderFactory`]
/// the sync engine drives).
#[derive(Clone)]
pub struct AppProviderFactory {
    secrets: SharedSecretStore,
}

impl AppProviderFactory {
    /// Creates a factory that resolves credentials through `secrets`.
    #[must_use]
    pub fn new(secrets: SharedSecretStore) -> Self {
        Self { secrets }
    }
}

impl ProviderFactory<AnyIncomingProvider> for AppProviderFactory {
    async fn make(&self, account: &Account) -> Result<AnyIncomingProvider, MailError> {
        match account.incoming.protocol {
            IncomingProtocol::Imap => {
                let password = resolve_secret(&self.secrets, &account.incoming.credential).await?;
                // Production trust: the webpki bundle only. `extra_root_certs_der`
                // stays empty and there is no code path that weakens verification.
                let config = ImapConfig::new(
                    account.incoming.endpoint.clone(),
                    account.incoming.security,
                    account.incoming.username.clone(),
                    password,
                );
                Ok(AnyIncomingProvider::Imap(ImapProvider::new(config)))
            }
            IncomingProtocol::Pop3 => Err(MailError::UnsupportedCapability {
                capability: "POP3 receiving (arrives in phase 3)".to_owned(),
            }),
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
