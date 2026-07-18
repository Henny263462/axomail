//! IMAP protocol adapter.
//!
//! Implements the incoming-provider trait from `axomail-core` for IMAP
//! (phase 2): UID-based two-way synchronization, capability detection,
//! mailbox listing, and flag sync. IMAP-specific types must not leak out
//! of this crate.
//!
//! # Security
//!
//! Every connection is TLS-protected: either implicit TLS or a `STARTTLS`
//! upgrade completed **before** credentials are sent. Certificate
//! verification is always on; [`ImapConfig::extra_root_certs_der`] can only
//! *add* trust anchors (for local test servers), never disable verification.
//! TLS failures map to [`axomail_core::MailError::Tls`] and are never
//! retried or downgraded.
//!
//! # Usage
//!
//! Construct an [`ImapProvider`] with an [`ImapConfig`], then drive it
//! through the [`IncomingMailProvider`](axomail_core::IncomingMailProvider)
//! trait: `connect`, then `discover`/`sync`, then per-message operations.
#![forbid(unsafe_code)]

mod config;
mod connection;
mod convert;
mod date;
mod discover;
mod error;
mod message_ops;
mod provider;
mod rfc2047;
mod sync;
mod tls;
mod utf7;

pub use config::{DEFAULT_TIMEOUT, ImapConfig};
pub use provider::ImapProvider;
