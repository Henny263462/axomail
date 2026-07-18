//! [`KeyringSecretStore`]: the operating-system keychain backend.

use axomail_core::{CredentialRef, MailError};
use keyring::{Entry, Error as KeyringError};

use crate::secret_store::SecretStore;

/// Keychain service name grouping every axomail secret.
///
/// The [`CredentialRef`] string is used as the account/user name within
/// this service, so the platform keychain shows entries such as
/// `axomail / account/1/incoming`.
const SERVICE: &str = "axomail";

/// [`SecretStore`] backed by the operating-system keychain via the
/// `keyring` crate.
///
/// On Windows this is the Credential Manager, on macOS the Keychain, and on
/// Linux the Secret Service. Secrets therefore never touch the SQLite
/// database, the log stream, or any serialized core type.
///
/// The type is zero-sized: an [`Entry`] is created per operation, which
/// keeps the store cheap to clone and free of shared mutable state.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    /// Creates a keychain-backed secret store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the keychain [`Entry`] for a credential reference.
    fn entry(reference: &CredentialRef) -> Result<Entry, MailError> {
        Entry::new(SERVICE, reference.as_str()).map_err(map_keyring_error)
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_secret(&self, reference: &CredentialRef, secret: &str) -> Result<(), MailError> {
        Self::entry(reference)?.set_password(secret).map_err(|err| {
            tracing::debug!(reference = reference.as_str(), "keychain write failed");
            map_keyring_error(err)
        })
    }

    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>, MailError> {
        match Self::entry(reference)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(err) => {
                tracing::debug!(reference = reference.as_str(), "keychain read failed");
                Err(map_keyring_error(err))
            }
        }
    }

    fn delete_secret(&self, reference: &CredentialRef) -> Result<(), MailError> {
        match Self::entry(reference)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => {
                tracing::debug!(reference = reference.as_str(), "keychain delete failed");
                Err(map_keyring_error(err))
            }
        }
    }
}

/// Maps a `keyring` error to a redacted [`MailError::Keychain`].
///
/// The platform error payload is deliberately discarded: raw OS keychain
/// errors can embed attribute values or item contents, so only a fixed,
/// secret-free description of the failure class is surfaced. `NoEntry` is
/// mapped here for completeness, but read and delete handle it as a normal
/// "absent" state before reaching this function.
fn map_keyring_error(err: KeyringError) -> MailError {
    let message = match err {
        KeyringError::NoEntry => "no credential exists for the reference",
        KeyringError::NoStorageAccess(_) => "the credential store is locked or inaccessible",
        KeyringError::PlatformFailure(_) => "the platform credential store reported a failure",
        KeyringError::BadEncoding(_) => "the stored credential was not valid UTF-8",
        KeyringError::BadDataFormat(_, _) => "the stored credential had an unexpected format",
        KeyringError::BadStoreFormat(_) => "the credential store had an unexpected format",
        KeyringError::TooLong(_, _) => "a credential attribute exceeded a platform length limit",
        KeyringError::Invalid(_, _) => "an invalid parameter was passed to the credential store",
        KeyringError::Ambiguous(_) => "multiple credentials matched the reference",
        KeyringError::NoDefaultStore => "no platform credential store is available",
        KeyringError::NotSupportedByStore(_) => {
            "the credential store does not support the operation"
        }
        // `keyring::Error` is `#[non_exhaustive]`; stay safe on new variants.
        _ => "the credential store reported an unspecified error",
    };
    MailError::Keychain {
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The redacted message for unit `keyring` variants must be a fixed,
    /// secret-free string and never expose internals.
    #[test]
    fn maps_unit_variants_to_redacted_keychain_errors() {
        for err in [KeyringError::NoEntry, KeyringError::NoDefaultStore] {
            let MailError::Keychain { message } = map_keyring_error(err) else {
                panic!("expected a Keychain error");
            };
            assert!(!message.is_empty());
            assert!(message.is_ascii());
            assert!(!message.contains("password"));
            assert!(!message.contains("secret"));
        }
    }
}
