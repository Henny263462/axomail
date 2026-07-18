//! The [`SecretStore`] abstraction.

use axomail_core::{CredentialRef, MailError};

/// Store that resolves a [`CredentialRef`] to the secret it references.
///
/// A [`CredentialRef`] is a non-secret handle (for example
/// `account/1/incoming`); the secret itself lives only inside the store and
/// never in the database, in logs, or in any core type. Production code uses
/// [`KeyringSecretStore`](crate::KeyringSecretStore), which is backed by the
/// operating-system keychain; tests use
/// [`InMemorySecretStore`](crate::InMemorySecretStore).
///
/// # Blocking
///
/// The keychain backend performs **blocking** I/O against the platform
/// credential store, so every method may block. The trait is intentionally
/// synchronous and the crate pulls in no async runtime; async callers must
/// run these methods on a blocking-safe context such as
/// `tokio::task::spawn_blocking`.
///
/// # Secrecy contract
///
/// Implementations must never place the stored secret into a returned
/// [`MailError`], and errors must not embed raw platform error strings that
/// could carry credential material (see [`MailError::Keychain`]).
pub trait SecretStore {
    /// Stores `secret` under `reference`, replacing any existing value.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Keychain`] if the store cannot be written.
    fn set_secret(&self, reference: &CredentialRef, secret: &str) -> Result<(), MailError>;

    /// Retrieves the secret stored under `reference`.
    ///
    /// Returns `Ok(None)` when no secret is stored for the reference; this
    /// is a normal "not configured yet" state, not an error.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Keychain`] if the store cannot be read.
    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>, MailError>;

    /// Deletes the secret stored under `reference`.
    ///
    /// Deletion is idempotent: removing a reference that does not exist
    /// succeeds, so callers need not check for existence first.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Keychain`] if the store cannot be modified.
    fn delete_secret(&self, reference: &CredentialRef) -> Result<(), MailError>;
}
