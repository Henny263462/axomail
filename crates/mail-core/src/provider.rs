//! Provider abstractions for incoming and outgoing mail (ADR-0004).
//!
//! Protocol adapters (`axomail-imap`, `axomail-pop3`, `axomail-smtp`)
//! implement these traits; the rest of the core depends only on them.
//! The traits use native `async fn` (desugared to `impl Future + Send`
//! so the engine can demand `Send` futures). They are deliberately not
//! object-safe: composition roots wrap concrete adapters in an enum
//! for dispatch (ADR-0004), keeping `async-trait` out of the tree.
//!
//! Adapters are stateless with respect to local storage: the sync
//! engine passes known state in via [`SyncRequest`] and persists the
//! returned [`SyncOutcome`].

use crate::error::MailError;
use crate::id::RemoteMessageId;
use crate::keyword::{FlagDelta, FlagSet};
use crate::mailbox::MailboxName;
use crate::provider_types::{
    AppendOutcome, OutgoingMessage, RawMessage, RemoteMailboxState, SendResult, SyncOutcome,
    SyncRequest,
};

/// A source of incoming mail (IMAP or POP3).
///
/// Implementations own their connection state; `&mut self` reflects
/// that a session is a stateful, single-consumer resource. Operations
/// a protocol cannot support (e.g. server-side flags on POP3) return
/// [`MailError::UnsupportedCapability`].
pub trait IncomingMailProvider: Send {
    /// Establishes the connection and authenticates.
    fn connect(&mut self) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Discovers the remote folder structure.
    fn discover(&mut self) -> impl Future<Output = Result<RemoteMailboxState, MailError>> + Send;

    /// Runs one incremental synchronization pass for a mailbox.
    fn sync(
        &mut self,
        request: SyncRequest,
    ) -> impl Future<Output = Result<SyncOutcome, MailError>> + Send;

    /// Fetches one complete raw message.
    fn fetch_message(
        &mut self,
        id: &RemoteMessageId,
    ) -> impl Future<Output = Result<RawMessage, MailError>> + Send;

    /// Selects `mailbox` as the target of subsequent per-message operations.
    ///
    /// The per-message ops (`store_flags`, `move_message`, `delete_message`)
    /// act on the mailbox the session most recently selected. `sync` selects
    /// implicitly, but the write-back queue replays ops for messages the engine
    /// is not currently syncing, so it selects explicitly through this method
    /// first. On IMAP this issues `SELECT` and records the mailbox's
    /// `UIDVALIDITY`; a protocol without server-side folders (POP3) returns
    /// [`MailError::UnsupportedCapability`].
    fn select_mailbox(
        &mut self,
        mailbox: &MailboxName,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Applies an additive flag change to a message on the server.
    ///
    /// The `delta` names only the flags and keywords to add (`STORE +FLAGS`)
    /// and remove (`STORE -FLAGS`); it never replaces the full flag set, so a
    /// write can never clobber flags or keywords the caller did not name. An
    /// empty delta is a no-op.
    fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Moves a message to another mailbox on the server.
    fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Deletes a message on the server.
    fn delete_message(
        &mut self,
        id: &RemoteMessageId,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Appends `raw` (a complete RFC 5322 message) to `mailbox` on the server,
    /// tagged with `flags` (e.g. `\Seen` for a Sent copy, `\Draft` for a saved
    /// draft).
    ///
    /// `mailbox` is the decoded full server path of the target folder; the
    /// adapter re-encodes it for the wire. This is plain RFC 3501 `APPEND`
    /// (capability-agnostic) and does not require the mailbox to be selected.
    /// The returned [`AppendOutcome`] carries the appended message's server
    /// identity when the adapter could resolve it, so the caller can adopt the
    /// server copy onto its local record. Protocols without server-side folders
    /// (POP3) return [`MailError::UnsupportedCapability`].
    fn append_message(
        &mut self,
        mailbox: &MailboxName,
        flags: &FlagSet,
        raw: &[u8],
    ) -> impl Future<Output = Result<AppendOutcome, MailError>> + Send;

    /// Creates a mailbox (folder) on the server at the given full path.
    ///
    /// `path` is the decoded full server path; the adapter re-encodes it for
    /// the wire (e.g. IMAP modified UTF-7). Protocols without server-side
    /// folders (POP3) return [`MailError::UnsupportedCapability`].
    fn create_mailbox(
        &mut self,
        path: &MailboxName,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Renames (and/or moves) a mailbox from one full path to another.
    fn rename_mailbox(
        &mut self,
        from: &MailboxName,
        to: &MailboxName,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Deletes a mailbox on the server.
    fn delete_mailbox(
        &mut self,
        path: &MailboxName,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Subscribes to (`subscribed = true`) or unsubscribes from
    /// (`subscribed = false`) a mailbox on the server, issuing `SUBSCRIBE` or
    /// `UNSUBSCRIBE` respectively.
    fn set_mailbox_subscribed(
        &mut self,
        path: &MailboxName,
        subscribed: bool,
    ) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Closes the connection gracefully.
    fn disconnect(&mut self) -> impl Future<Output = Result<(), MailError>> + Send;
}

/// A sink for outgoing mail (SMTP submission).
pub trait OutgoingMailProvider: Send + Sync {
    /// Submits one message.
    fn send(
        &self,
        message: &OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult, MailError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Address;
    use crate::id::MailboxId;
    use crate::provider_types::MailboxSyncState;

    /// Compile-time proof that both traits are implementable with
    /// plain `async fn` and that their futures are `Send`.
    struct NullIncoming;

    impl IncomingMailProvider for NullIncoming {
        async fn connect(&mut self) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
            Err(MailError::Cancelled)
        }

        async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
            Err(MailError::Cancelled)
        }

        async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
            Err(MailError::Cancelled)
        }

        async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn store_flags(
            &mut self,
            _id: &RemoteMessageId,
            _delta: &FlagDelta,
        ) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn move_message(
            &mut self,
            _id: &RemoteMessageId,
            _target: &MailboxName,
        ) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn append_message(
            &mut self,
            _mailbox: &MailboxName,
            _flags: &FlagSet,
            _raw: &[u8],
        ) -> Result<AppendOutcome, MailError> {
            Err(MailError::Cancelled)
        }

        async fn create_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn rename_mailbox(
            &mut self,
            _from: &MailboxName,
            _to: &MailboxName,
        ) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn set_mailbox_subscribed(
            &mut self,
            _path: &MailboxName,
            _subscribed: bool,
        ) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn disconnect(&mut self) -> Result<(), MailError> {
            Ok(())
        }
    }

    struct NullOutgoing;

    impl OutgoingMailProvider for NullOutgoing {
        async fn send(&self, _message: &OutgoingMessage) -> Result<SendResult, MailError> {
            Err(MailError::Cancelled)
        }
    }

    fn assert_send_future<F: Future + Send>(_f: F) {}

    #[test]
    fn incoming_provider_futures_are_send() {
        let mut provider = NullIncoming;
        assert_send_future(provider.connect());
        assert_send_future(provider.discover());
        assert_send_future(provider.sync(SyncRequest {
            mailbox: MailboxId::new(1),
            remote_name: MailboxName::new("INBOX"),
            state: MailboxSyncState::default(),
            batch_size: 100,
            full_reconcile: false,
        }));
        assert_send_future(provider.select_mailbox(&MailboxName::new("INBOX")));
        let delta = FlagDelta::default();
        assert_send_future(provider.store_flags(
            &RemoteMessageId::Imap {
                uid: 1,
                uid_validity: 1,
            },
            &delta,
        ));
        let flags = FlagSet::default();
        assert_send_future(provider.append_message(&MailboxName::new("Sent"), &flags, b"raw"));
        assert_send_future(provider.disconnect());
    }

    #[test]
    fn outgoing_provider_futures_are_send() {
        let provider = NullOutgoing;
        let message = OutgoingMessage {
            from: Address {
                display_name: None,
                email: crate::EmailAddress::parse("a@example.org")
                    .unwrap_or_else(|_| unreachable!("static test address is valid")),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            reply_to: None,
            subject: String::new(),
            text_body: String::new(),
            html_body: None,
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        };
        assert_send_future(provider.send(&message));
    }
}
