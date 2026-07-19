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
use crate::keyword::FlagDelta;
use crate::mailbox::MailboxName;
use crate::provider_types::{
    OutgoingMessage, RawMessage, RemoteMailboxState, SendResult, SyncOutcome, SyncRequest,
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
        }));
        let delta = FlagDelta::default();
        assert_send_future(provider.store_flags(
            &RemoteMessageId::Imap {
                uid: 1,
                uid_validity: 1,
            },
            &delta,
        ));
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
        };
        assert_send_future(provider.send(&message));
    }
}
