//! Security layer.
//!
//! This crate concentrates the security-sensitive primitives of axomail so
//! that the rest of the workspace never has to reason about them ad hoc:
//!
//! - the [`SecretStore`] abstraction over the operating-system keychain,
//!   with a production [`KeyringSecretStore`] and a test-only
//!   [`InMemorySecretStore`];
//! - stable [`CredentialRef`](axomail_core::CredentialRef) construction for
//!   an account via [`credential_ref_for_account`];
//! - [`sanitize_attachment_filename`], a pure defence against path
//!   traversal, control characters, and reserved device names when writing
//!   attachments to disk;
//! - [`redact_email`] and [`redact_secretlike`], conservative helpers that
//!   keep addresses and token-shaped strings out of logs.
//!
//! HTML sanitization for untrusted message content will also live here, but
//! is deferred to phase 2 (see ADR-0007).
//!
//! # Blocking I/O
//!
//! Keychain access is synchronous and blocking. This crate deliberately
//! does not depend on an async runtime; async callers must offload
//! [`SecretStore`] calls to a blocking-safe context (for example
//! `tokio::task::spawn_blocking`).
#![forbid(unsafe_code)]

mod credential;
mod filename;
mod keyring_store;
mod memory_store;
mod redact;
mod secret_store;

pub use credential::{Direction, credential_ref_for_account};
pub use filename::sanitize_attachment_filename;
pub use keyring_store::KeyringSecretStore;
pub use memory_store::InMemorySecretStore;
pub use redact::{redact_email, redact_secretlike};
pub use secret_store::SecretStore;
