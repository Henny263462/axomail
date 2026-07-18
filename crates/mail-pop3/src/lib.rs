//! POP3 protocol adapter.
//!
//! Implements the incoming-provider trait from `axomail-core` for POP3
//! (phase 3). The client is hand-rolled on tokio + tokio-rustls (ADR-0006):
//! the protocol is a dozen line-oriented commands and keeping it in-house
//! puts every `UIDL` edge case under our control.
//!
//! POP3 is treated strictly as a **download source** (ADR-0001): there are
//! no server-side folders or flags, deduplication is `UIDL`-based, and
//! server-side deletion happens only under a doubly guarded user policy.
//!
//! # Security
//!
//! Every connection is TLS-protected: either implicit TLS or an `STLS`
//! upgrade completed **before** credentials are sent. If a `StartTls`
//! account's server does not offer `STLS`, the adapter fails with
//! [`axomail_core::MailError::UnsupportedCapability`] rather than continuing
//! in plaintext. Certificate verification is always on;
//! [`Pop3Config::extra_root_certs_der`] can only *add* trust anchors (for
//! local test servers), never disable verification. TLS failures map to
//! [`axomail_core::MailError::Tls`] and are never retried or downgraded.
//!
//! # Identity and sync
//!
//! POP3 has no `UIDVALIDITY`, so [`axomail_core::SyncOutcome`] always carries
//! `uid_validity: None`. Messages are identified by their server `UIDL`
//! ([`axomail_core::RemoteMessageId::Pop3`]); on servers without `UIDL` the
//! adapter falls back to a content digest (see the module docs on stability
//! risks). Because [`axomail_core::SyncRequest`] cannot carry the set of
//! already-known `UIDL`s, the composition layer supplies it via
//! [`Pop3Provider::set_known_uidls`] before each sync.
#![forbid(unsafe_code)]

mod command;
mod config;
mod connection;
mod date;
mod error;
mod headers;
mod provider;
mod sync;
mod tls;
mod wire;

pub use config::{DEFAULT_TIMEOUT, Pop3Config};
pub use provider::Pop3Provider;
