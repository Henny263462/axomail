//! Core domain layer of axomail.
//!
//! Holds typed identifiers, the central [`MailError`] hierarchy, domain
//! models (accounts, mailboxes, message envelopes), and the provider
//! traits that protocol adapters implement. Contains no protocol,
//! storage, or UI code.
//!
//! Design rules (see ADR-0001, ADR-0003, ADR-0004):
//! - no bare `String`/`i64` identifiers in public APIs — always newtypes
//! - errors never contain secrets and are classified for retry decisions
//! - provider traits use native `async fn`; dyn dispatch is replaced by
//!   enum dispatch in `mail-sync`
#![forbid(unsafe_code)]

mod account;
mod address;
mod error;
mod id;
mod mailbox;
mod message;
mod provider;
mod provider_types;

pub use account::{
    Account, ConnectionSecurity, IncomingProtocol, IncomingServerConfig, OutgoingServerConfig,
    ServerEndpoint,
};
pub use address::{Address, EmailAddress};
pub use error::{ErrorClass, MailError};
pub use id::{AccountId, AttachmentId, CredentialRef, MailboxId, MessageId, RemoteMessageId};
pub use mailbox::{Mailbox, MailboxName, MailboxRole};
pub use message::{MessageEnvelope, MessageFlags, UtcTimestamp};
pub use provider::{IncomingMailProvider, OutgoingMailProvider};
pub use provider_types::{
    MailboxSyncState, OutgoingAttachment, OutgoingMessage, RawMessage, RemoteEnvelope,
    RemoteMailbox, RemoteMailboxState, SendResult, SyncOutcome, SyncRequest, SyncSummary,
};
