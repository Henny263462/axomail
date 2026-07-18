//! Provider abstractions for incoming and outgoing mail (ADR-0004).
//!
//! Protocol adapters (`axomail-imap`, `axomail-pop3`, `axomail-smtp`)
//! implement these traits; the rest of the core depends only on them.
//! The traits use native `async fn`. They are deliberately **not**
//! object-safe: `mail-sync` wraps concrete adapters in an enum for
//! dispatch instead of using `dyn` (see ADR-0004), which keeps the
//! `async-trait` crate out of the dependency tree.
//!
//! The request/response types below are the minimal shapes needed to
//! define the boundary; they will grow with the phase-2 adapters.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::error::MailError;
use crate::id::{MailboxId, RemoteMessageId};
use crate::mailbox::MailboxName;

/// A mailbox as reported by the remote server during discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMailbox {
    /// Decoded mailbox name.
    pub name: MailboxName,
}

/// Snapshot of the remote folder structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMailboxState {
    /// All selectable mailboxes on the server.
    pub mailboxes: Vec<RemoteMailbox>,
}

/// Instruction for one synchronization pass over one mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Local mailbox to synchronize.
    pub mailbox: MailboxId,
}

/// Outcome of one synchronization pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSummary {
    /// Messages newly seen on the server.
    pub new_messages: u64,
    /// Messages whose flags changed.
    pub updated_flags: u64,
    /// Messages no longer present on the server.
    pub removed_messages: u64,
}

/// A raw, unparsed message as fetched from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    /// Identity of the message on the server.
    pub remote_id: RemoteMessageId,
    /// Complete RFC 5322 byte stream.
    pub bytes: Vec<u8>,
}

/// A message to be submitted via the outgoing provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Sender address.
    pub from: Address,
    /// Primary recipients; must not be empty at send time.
    pub to: Vec<Address>,
    /// Carbon-copy recipients.
    pub cc: Vec<Address>,
    /// Blind-carbon-copy recipients (never serialized into headers by
    /// the SMTP adapter).
    pub bcc: Vec<Address>,
    /// Optional reply-to address.
    pub reply_to: Option<Address>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub text_body: String,
}

/// Acknowledgment for a submitted message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendResult {
    /// Redacted final server reply line (e.g. the `250` acknowledgment),
    /// kept for diagnostics.
    pub server_reply: Option<String>,
}

/// A source of incoming mail (IMAP or POP3).
///
/// Implementations own their connection state; `&mut self` reflects
/// that a session is a stateful, single-consumer resource.
pub trait IncomingMailProvider: Send {
    /// Establishes the connection and authenticates.
    fn connect(&mut self) -> impl Future<Output = Result<(), MailError>> + Send;

    /// Discovers the remote folder structure.
    fn discover(&mut self) -> impl Future<Output = Result<RemoteMailboxState, MailError>> + Send;

    /// Runs one synchronization pass for a mailbox.
    fn sync(
        &mut self,
        request: SyncRequest,
    ) -> impl Future<Output = Result<SyncSummary, MailError>> + Send;

    /// Fetches one complete raw message.
    fn fetch_message(
        &mut self,
        id: &RemoteMessageId,
    ) -> impl Future<Output = Result<RawMessage, MailError>> + Send;

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

    /// Compile-time proof that the traits are implementable with plain
    /// `async fn` and that their futures are `Send`.
    struct NullIncoming;

    impl IncomingMailProvider for NullIncoming {
        async fn connect(&mut self) -> Result<(), MailError> {
            Err(MailError::Cancelled)
        }

        async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
            Err(MailError::Cancelled)
        }

        async fn sync(&mut self, _request: SyncRequest) -> Result<SyncSummary, MailError> {
            Err(MailError::Cancelled)
        }

        async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
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
        }));
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
        };
        assert_send_future(provider.send(&message));
    }
}
