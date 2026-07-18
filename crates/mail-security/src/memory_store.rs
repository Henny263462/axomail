//! [`InMemorySecretStore`]: a process-local backend for tests.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use axomail_core::{CredentialRef, MailError};

use crate::secret_store::SecretStore;

/// [`SecretStore`] that keeps secrets in a process-local map.
///
/// Intended for unit and integration tests and for headless environments
/// without a platform keychain. Secrets live only in memory behind a
/// [`Mutex`] and are never persisted.
///
/// This type provides none of the protections of the operating-system
/// keychain and must not be used in production.
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    entries: Mutex<HashMap<String, String>>,
}

impl InMemorySecretStore {
    /// Creates an empty in-memory secret store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Locks the map, mapping a poisoned lock to a keychain error rather
    /// than panicking (production paths must not panic on expectable
    /// failures).
    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, String>>, MailError> {
        self.entries.lock().map_err(|_| MailError::Keychain {
            message: "in-memory secret store lock was poisoned".to_owned(),
        })
    }
}

impl SecretStore for InMemorySecretStore {
    fn set_secret(&self, reference: &CredentialRef, secret: &str) -> Result<(), MailError> {
        self.lock()?
            .insert(reference.as_str().to_owned(), secret.to_owned());
        Ok(())
    }

    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>, MailError> {
        Ok(self.lock()?.get(reference.as_str()).cloned())
    }

    fn delete_secret(&self, reference: &CredentialRef) -> Result<(), MailError> {
        self.lock()?.remove(reference.as_str());
        Ok(())
    }
}
