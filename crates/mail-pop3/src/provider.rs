//! The [`Pop3Provider`] adapter implementing [`IncomingMailProvider`].

use std::collections::HashSet;

use axomail_core::{
    FlagDelta, IncomingMailProvider, MailError, MailboxName, MailboxRole, RawMessage,
    RemoteMailbox, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};

use crate::command;
use crate::config::Pop3Config;
use crate::connection::{self, Connected};
use crate::sync::{self, SessionMap};

/// A POP3 incoming-mail adapter.
///
/// A single instance owns one connection and treats POP3 strictly as a
/// download source (ADR-0001): there are no server-side folders or flags,
/// deduplication is `UIDL`-based, and server-side deletion is doubly guarded.
///
/// # Lifecycle
///
/// Call [`connect`](IncomingMailProvider::connect) first, then
/// [`discover`](IncomingMailProvider::discover) and
/// [`sync`](IncomingMailProvider::sync). Per-message operations
/// (`fetch_message`, `delete_message`) act on the number map built by the
/// most recent `sync` on the current connection.
///
/// # Known `UIDL`s
///
/// [`SyncRequest`] cannot carry the set of `UIDL`s the engine already knows
/// (POP3 has no incremental cursor), so the composition layer must call
/// [`set_known_uidls`](Self::set_known_uidls) before each sync. Messages in
/// that set are skipped; everything else is reported as new.
pub struct Pop3Provider {
    config: Pop3Config,
    connected: Option<Connected>,
    known_uidls: HashSet<String>,
    session: Option<SessionMap>,
}

impl Pop3Provider {
    /// Creates a provider for `config` without opening a connection.
    #[must_use]
    pub fn new(config: Pop3Config) -> Self {
        Self {
            config,
            connected: None,
            known_uidls: HashSet::new(),
            session: None,
        }
    }

    /// Replaces the set of `UIDL`s the engine already has in local storage.
    ///
    /// The composition layer fills this from storage before every
    /// [`sync`](IncomingMailProvider::sync): POP3 has no `UIDVALIDITY` and no
    /// server-side cursor, so the adapter cannot otherwise tell which
    /// messages are new. Any `UIDL` present here is treated as already
    /// ingested and omitted from the sync's `new_envelopes`; `UIDL`s absent
    /// from the server but present here are reported as removed.
    pub fn set_known_uidls(&mut self, uidls: HashSet<String>) {
        self.known_uidls = uidls;
    }

    /// Returns the live session, or a configuration error if not connected.
    fn require_connected(&mut self) -> Result<&mut Connected, MailError> {
        self.connected
            .as_mut()
            .ok_or_else(|| MailError::Configuration {
                message: "the POP3 session is not connected; call connect first".to_owned(),
            })
    }

    /// Resolves a remote id to the current session's message number.
    fn number_for(&self, id: &RemoteMessageId) -> Result<u32, MailError> {
        let uidl = pop3_uidl(id)?;
        let session = self.session.as_ref().ok_or_else(|| MailError::Protocol {
            message: "sync a mailbox before per-message operations".to_owned(),
        })?;
        session.number_for(uidl).ok_or_else(|| MailError::Protocol {
            message: "the message is unknown in this session; re-sync first".to_owned(),
        })
    }
}

impl IncomingMailProvider for Pop3Provider {
    async fn connect(&mut self) -> Result<(), MailError> {
        let connected = connection::connect(&self.config).await?;
        self.connected = Some(connected);
        self.session = None;
        Ok(())
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        // POP3 exposes a single implicit mailbox; there is nothing to list.
        Ok(RemoteMailboxState {
            mailboxes: vec![RemoteMailbox {
                name: MailboxName::new("INBOX"),
                role: Some(MailboxRole::Inbox),
            }],
        })
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        let timeout = self.config.timeout;
        let known = self.known_uidls.clone();
        let connected = self.require_connected()?;
        let (outcome, session) = sync::run(connected, &request, &known, timeout).await?;
        self.session = Some(session);
        Ok(outcome)
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        let number = self.number_for(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_connected()?;
        let bytes = command::retr(connected.wire(), number, timeout).await?;
        Ok(RawMessage {
            remote_id: id.clone(),
            bytes,
        })
    }

    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        // POP3 exposes a single implicit mailbox and has no SELECT.
        Err(MailError::UnsupportedCapability {
            capability: "mailbox selection (POP3 is download-only)".to_owned(),
        })
    }

    async fn store_flags(
        &mut self,
        _id: &RemoteMessageId,
        _delta: &FlagDelta,
    ) -> Result<(), MailError> {
        // POP3 has no server-side flags; flags are a local-only concept.
        Err(MailError::UnsupportedCapability {
            capability: "server-side flags (POP3 is download-only)".to_owned(),
        })
    }

    async fn move_message(
        &mut self,
        _id: &RemoteMessageId,
        _target: &MailboxName,
    ) -> Result<(), MailError> {
        // POP3 has no server-side folders to move between.
        Err(MailError::UnsupportedCapability {
            capability: "server-side move (POP3 is download-only)".to_owned(),
        })
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        // First guard: without the account policy the adapter never deletes.
        // The command layer is the second guard and must have confirmed the
        // user's intent before reaching this call (see `Pop3Config`).
        if !self.config.delete_after_download {
            return Err(MailError::UnsupportedCapability {
                capability: "server-side delete (enable delete_after_download and confirm)"
                    .to_owned(),
            });
        }
        let number = self.number_for(id)?;
        let timeout = self.config.timeout;
        let connected = self.require_connected()?;
        // `DELE` only marks the message; it is committed by `QUIT` in
        // `disconnect`.
        command::dele(connected.wire(), number, timeout).await
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        self.session = None;
        if let Some(connected) = self.connected.take() {
            let timeout = self.config.timeout;
            let mut wire = connected.into_wire();
            // Best-effort `QUIT`: it commits any pending `DELE`, but a broken
            // pipe here is not an error since the connection is closing.
            let _ = command::quit(&mut wire, timeout).await;
        }
        Ok(())
    }
}

/// Extracts the `UIDL` from a POP3 remote id, rejecting foreign identities.
fn pop3_uidl(id: &RemoteMessageId) -> Result<&str, MailError> {
    match id {
        RemoteMessageId::Pop3 { uidl } => Ok(uidl),
        RemoteMessageId::Imap { .. } => Err(MailError::Protocol {
            message: "an IMAP message identity was given to the POP3 adapter".to_owned(),
        }),
    }
}
