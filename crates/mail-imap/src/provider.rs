//! The [`ImapProvider`] adapter implementing [`IncomingMailProvider`].

use std::time::Duration;

use axomail_core::{
    AppendOutcome, FlagDelta, FlagSet, IdleOutcome, IncomingMailProvider, MailError, MailboxName,
    RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};

use crate::config::ImapConfig;
use crate::connection::{self, Connected};
use crate::convert::flag_set_to_imap_list;
use crate::error::{map_imap_error, with_deadline};
use crate::{append, discover, folder_ops, idle, message_ops, sync, utf7};

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

    async fn select_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(mailbox.as_str());
        let connected = self.require_session()?;
        let uid_validity = message_ops::select(connected, &encoded, timeout).await?;
        self.selected = Some(SelectedMailbox { uid_validity });
        Ok(())
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

    async fn copy_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let encoded_target = utf7::encode(target.as_str());
        let connected = self.require_session()?;
        message_ops::copy_to(connected, uid, &encoded_target, timeout).await
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        let uid = self.resolve_uid(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_session()?;
        message_ops::delete(connected, uid, timeout).await
    }

    async fn expunge_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(mailbox.as_str());
        let connected = self.require_session()?;
        // SELECT first so the subsequent EXPUNGE is confined to this mailbox
        // (see `message_ops::empty_selected`).
        let uid_validity = message_ops::select(connected, &encoded, timeout).await?;
        self.selected = Some(SelectedMailbox { uid_validity });
        let connected = self.require_session()?;
        message_ops::empty_selected(connected, timeout).await
    }

    async fn append_message(
        &mut self,
        mailbox: &MailboxName,
        flags: &FlagSet,
        raw: &[u8],
    ) -> Result<AppendOutcome, MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(mailbox.as_str());
        let flag_list = (!flags.is_empty()).then(|| flag_set_to_imap_list(flags));
        let message_id = append::parse_message_id(raw);
        let connected = self.require_session()?;
        let remote_id = append::append(
            connected,
            &encoded,
            flag_list.as_deref(),
            raw,
            message_id.as_deref(),
            timeout,
        )
        .await?;
        // UID resolution selects the target mailbox; reflect that so a later
        // per-message op on this session resolves against the right validity.
        if let Some(RemoteMessageId::Imap { uid_validity, .. }) = remote_id {
            self.selected = Some(SelectedMailbox { uid_validity });
        }
        Ok(AppendOutcome { remote_id })
    }

    async fn create_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(path.as_str());
        let connected = self.require_session()?;
        folder_ops::create(connected, &encoded, timeout).await
    }

    async fn rename_mailbox(
        &mut self,
        from: &MailboxName,
        to: &MailboxName,
    ) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded_from = utf7::encode(from.as_str());
        let encoded_to = utf7::encode(to.as_str());
        let connected = self.require_session()?;
        folder_ops::rename(connected, &encoded_from, &encoded_to, timeout).await
    }

    async fn delete_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(path.as_str());
        let connected = self.require_session()?;
        folder_ops::delete(connected, &encoded, timeout).await
    }

    async fn set_mailbox_subscribed(
        &mut self,
        path: &MailboxName,
        subscribed: bool,
    ) -> Result<(), MailError> {
        let timeout = self.config.timeout;
        let encoded = utf7::encode(path.as_str());
        let connected = self.require_session()?;
        folder_ops::set_subscribed(connected, &encoded, subscribed, timeout).await
    }

    fn supports_idle(&self) -> bool {
        self.connected
            .as_ref()
            .is_some_and(|connected| connected.has_capability("IDLE"))
    }

    async fn idle(
        &mut self,
        max_wait: Duration,
        interrupt: impl Future<Output = ()> + Send,
    ) -> Result<IdleOutcome, MailError> {
        let timeout = self.config.timeout;
        // Take the session out for the duration of the wait: async-imap's IDLE
        // handle consumes the session by value and hands it back on `DONE`.
        let connected = self
            .connected
            .take()
            .ok_or_else(|| MailError::Configuration {
                message: "the IMAP session is not connected; call connect first".to_owned(),
            })?;
        // On error the session is not returned (the connection is broken), so
        // `self.connected` stays `None` and the caller reconnects.
        let (connected, outcome) = idle::run(connected, max_wait, interrupt, timeout).await?;
        self.connected = Some(connected);
        Ok(outcome)
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
