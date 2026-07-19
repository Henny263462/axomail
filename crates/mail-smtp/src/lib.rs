//! SMTP submission adapter.
//!
//! Implements [`axomail_core::OutgoingMailProvider`] on top of
//! [`mail-send`](mail_send) (transport, AUTH, TLS/STARTTLS) and
//! [`mail-builder`](mail_builder) (MIME assembly). The adapter is stateless:
//! [`SmtpProvider`]'s [`send`](axomail_core::OutgoingMailProvider::send) opens a
//! fresh authenticated connection, submits one message, and closes it.
//!
//! # TLS trust configuration
//!
//! mail-send builds its default TLS connector from the operating-system trust
//! store (via `rustls-platform-verifier`) and its `SmtpClientBuilder::new`
//! constructor exposes an `allow_invalid_certs` escape hatch. axomail wants
//! neither: trust is pinned to the [`webpki-roots`](webpki_roots) bundle plus,
//! **for the local test server only**, any DER roots supplied through
//! [`SmtpConfig::extra_root_certs_der`].
//!
//! mail-send's builder *can* take a custom root store, because every field of
//! its `SmtpClientBuilder` — including `tls_connector` — is public. This
//! adapter therefore constructs its own [`tokio_rustls::TlsConnector`] from a
//! [`rustls::ClientConfig`] with ordinary certificate verification and assigns
//! it directly. The `allow_invalid_certs` builder method is never
//! called, and it is unreachable from [`SmtpConfig`]: there is no field, flag,
//! or code path that disables certificate validation. Certificate and handshake
//! failures map to [`axomail_core::MailError::Tls`] and are never downgraded to
//! plaintext nor retried (ADR-0003).
//!
//! # Redaction
//!
//! Server reply lines can echo recipient addresses, so the final reply returned
//! in [`axomail_core::SendResult`] and any reply text folded into an error are
//! redacted first: no full address and no secret-shaped token survives into
//! diagnostics.
#![forbid(unsafe_code)]

mod config;
mod error_map;
mod mime;
mod provider;
mod redact;
mod tls;

pub use config::{DEFAULT_TIMEOUT, SmtpConfig};
pub use mime::{RenderedMessage, render_message};
pub use provider::SmtpProvider;
