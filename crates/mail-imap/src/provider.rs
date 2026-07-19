//! The [`ImapProvider`] adapter implementing [`IncomingMailProvider`].

use axomail_core::{
    FlagDelta, IncomingMailProvider, MailError, MailboxName, RawMessage, RemoteMailboxState,
    RemoteMessageId, SyncOutcome, SyncRequest,
};

use crate::config::ImapConfig;
use crate::connection::{self, Connected};
use crate::error::{map_imap_error, with_deadline};
use crate::{discover, message_ops, sync, utf7};

/// Records which mailbox the session currently has selected, so that
/// per-message operations can validate the caller's message identity.
struct SelectedMailbox {
    /// `UIDVALIDITY` of the selected mailbox at selection time.
    uid_validity: u32,
}

/// An IMAP incoming-mail adapter.
///
/// A single instance owns one connection. Call [`IncomingMailProvider::connect`]
/// first; then [`discover`](IncomingMailProvider::discover) and
/// [`sync`](IncomingMailProvider::sync). Per-message operations
/// (`fetch_message`, `store_flags`, `move_message`, `delete_message`) act on
/// the mailbox most recently selected by `sync`, and require the message's
/// `uid_validity` to match that mailbox.
pub struct ImapProvider {
    config: ImapConfig,
    connected: Option<Connected>,
    selected: Option<SelectedMailbox>,
}

impl ImapProvider {
    /// Creates a provider for `config` without opening a connection.
    #[must_use]
    pub fn new(config: ImapConfig) -> Self {
        Self {
            config,
            connected: None,
            selected: None,
        }
    }

    /// Returns the live session, or a configuration error if not connected.
    fn require_session(&mut self) -> Result<&mut Connected, MailError> {
        self.connected
            .as_mut()
            .ok_or_else(|| MailError::Configuration {
                message: "the IMAP session is not connected; call connect first".to_owned(),
            })
    }

    /// Validates that `id` is an IMAP identity for the selected mailbox and
    /// returns its UID.
    fn resolve_uid(&self, id: &RemoteMessageId) -> Result<u32, MailError> {
        let (uid, uid_validity) = match id {
            RemoteMessageId::Imap { uid, uid_validity } => (*uid, *uid_validity),
            RemoteMessageId::Pop3 { .. } => {
                return Err(MailError::Protocol {
                    message: "a POP3 message identity was given to the IMAP adapter".to_owned(),
                });
            }
        };
        match &self.selected {
            Some(selected) if selected.uid_validity == uid_validity => Ok(uid),
            Some(_) => Err(MailError::Protocol {
                message: "the message belongs to a mailbox with a different UIDVALIDITY; re-sync"
                    .to_owned(),
            }),
            None => Err(MailError::Protocol {
                message: "no mailbox is selected; sync a mailbox before per-message operations"
                    .to_owned(),
            }),
        }
    }
}

impl IncomingMailProvider for ImapProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        let connected = connection::connect(&self.config).await?;
        self.connected = Some(connected);
        self.selected = None;
        Ok(())
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        let timeout = self.config.timeout;
        let connected = self.require_session()?;
        discover::discover(connected, timeout).await
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        let timeout = self.config.timeout;
        let outcome = {
            let connected = self.require_session()?;
            sync::run(connected, &request, timeout).await?
        };
        if let Some(uid_validity) = outcome.uid_validity {
            self.selected = Some(SelectedMailbox { uid_validity });
        }
        Ok(outcome)
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_session()?;
        let bytes = message_ops::fetch_body(connected, uid, timeout).await?;
        Ok(RawMessage {
            remote_id: id.clone(),
            bytes,
        })
    }

    async fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> Result<(), MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_session()?;
        message_ops::store_flags(connected, uid, delta, timeout).await
    }

    async fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let encoded_target = utf7::encode(target.as_str());
        let connected = self.require_session()?;
        message_ops::move_to(connected, uid, &encoded_target, timeout).await
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_session()?;
        message_ops::delete(connected, uid, timeout).await
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        self.selected = None;
        if let Some(mut connected) = self.connected.take() {
            let timeout = self.config.timeout;
            // A best-effort LOGOUT: a broken pipe here is not an error, the
            // connection is being torn down regardless.
            let _ = with_deadline(timeout, async {
                connected
                    .session
                    .logout()
                    .await
                    .map_err(|err| map_imap_error(&err))
            })
            .await;
        }
        Ok(())
    }
}
