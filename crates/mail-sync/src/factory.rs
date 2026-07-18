//! Provider construction, decoupled from concrete protocol crates.
//!
//! The engine never names `axomail-imap` or `axomail-pop3`: it holds a
//! [`ProviderFactory`] supplied by the composition root (the app layer),
//! which builds a connected-capable provider for a given account. This is
//! where ADR-0004's dispatch enum is expected to live in the app, keeping
//! this crate protocol-agnostic.

use axomail_core::{Account, IncomingMailProvider, MailError};

/// Builds an [`IncomingMailProvider`] for an account on demand.
///
/// A fresh provider is created per job attempt so a transient failure
/// leads to a clean reconnect rather than reusing a dead session. The
/// factory returns the provider un-connected; the engine drives
/// [`IncomingMailProvider::connect`] itself so it can apply timeouts and
/// cancellation uniformly.
pub trait ProviderFactory<P>: Send + Sync + 'static
where
    P: IncomingMailProvider,
{
    /// Constructs a provider for `account`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError`] if the provider cannot be constructed (for
    /// example, an unresolvable credential reference).
    fn make(&self, account: &Account) -> impl Future<Output = Result<P, MailError>> + Send;
}
