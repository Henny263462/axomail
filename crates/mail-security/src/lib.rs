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
//!   keep addresses and token-shaped strings out of logs;
//! - [`sanitize_message_html`] (and its byte-level counterpart
//!   [`sanitize_message_html_bytes`]), the allowlist-based HTML sanitizer
//!   for untrusted message bodies and layer one of the four-layer rendering
//!   defense in ADR-0007. [`sanitize_message_html_with_policy`] takes a
//!   [`RemoteContentPolicy`] to keep remote images for an allowed sender;
//!   [`sanitize_message_html_with_inline`] additionally embeds message-local
//!   `cid:` images as `data:` URIs via an [`InlineImages`] map;
//!   [`detect_scam_indicators`] adds a warning-only deceptive-link pass.
//!
//! # Blocking I/O
//!
//! Keychain access is synchronous and blocking. This crate deliberately
//! does not depend on an async runtime; async callers must offload
//! [`SecretStore`] calls to a blocking-safe context (for example
//! `tokio::task::spawn_blocking`).
#![forbid(unsafe_code)]

mod active_content;
mod credential;
mod filename;
mod html;
mod html_policy;
mod inline_images;
mod keyring_store;
mod memory_store;
mod oauth_tokens;
mod redact;
mod remote_policy;
mod scam;
mod secret_store;

pub use credential::{Direction, credential_ref_for_account, credential_ref_for_oauth};
pub use filename::sanitize_attachment_filename;
pub use html::{
    SanitizedHtml, sanitize_message_html, sanitize_message_html_bytes,
    sanitize_message_html_with_inline, sanitize_message_html_with_policy,
};
pub use inline_images::{InlineImagePart, InlineImages, MAX_INLINE_IMAGE_BYTES};
pub use keyring_store::KeyringSecretStore;
pub use memory_store::InMemorySecretStore;
pub use oauth_tokens::OAuthTokens;
pub use redact::{redact_email, redact_secretlike};
pub use remote_policy::RemoteContentPolicy;
pub use scam::{ScamIndicator, ScamIndicatorKind, detect_scam_indicators};
pub use secret_store::SecretStore;
