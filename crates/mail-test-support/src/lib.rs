//! Shared test infrastructure for the axomail workspace.
//!
//! This crate provides reusable building blocks for integration tests
//! across the workspace (POP3, end-to-end sync, restart scenarios). It is
//! consumed only as a dev-dependency and never ships in release builds.
//!
//! The pieces fit together as follows:
//!
//! - [`ScriptedServer`] binds an ephemeral loopback port and serves a
//!   scripted dialog to each connection from a background task, recording a
//!   [`Transcript`] per connection. Dropping the server aborts its task and
//!   every connection it spawned, so tests never leak work.
//! - A dialog is a `Vec<`[`Step`]`>`: a tolerant, line-oriented script of
//!   expectations and responses (see [`script`]).
//! - [`TestTlsIdentity`] mints a self-signed identity so the same scripts
//!   can run over TLS; clients trust it via
//!   [`TestTlsIdentity::root_der`] or [`TestTlsIdentity::client_config`].
//! - [`imap`], [`smtp`], and [`pop3`] offer thin builders for the most
//!   common dialogs; callers can always hand-write [`Step`]s instead.
//! - [`fixtures`] holds realistic RFC 5322 messages for parser and adapter
//!   tests.
#![forbid(unsafe_code)]

mod error;
mod script;
mod server;
mod tls;

pub mod fixtures;
pub mod imap;
pub mod pop3;
pub mod smtp;

pub use error::TestServerError;
pub use script::{Step, Transcript};
pub use server::ScriptedServer;
pub use tls::TestTlsIdentity;
