//! Integration test against the real operating-system keychain.
//!
//! This is the only test that talks to `KeyringSecretStore` end to end. It
//! is `#[ignore]`d so it never runs in CI: it mutates the developer's actual
//! platform credential store (Windows Credential Manager, macOS Keychain, or
//! the Linux Secret Service). CI runners are typically headless and have no
//! unlocked keychain, so the call would either fail or block, and any entry
//! it created would persist on a shared machine. Run it locally with:
//!
//! ```text
//! cargo test -p axomail-security --test keyring_integration -- --ignored
//! ```
#![allow(clippy::unwrap_used)]

use axomail_core::CredentialRef;
use axomail_security::{KeyringSecretStore, SecretStore};

#[test]
#[ignore = "touches the real OS keychain; headless/unavailable on CI runners"]
fn round_trips_against_the_real_keychain() {
    let store = KeyringSecretStore::new();
    let reference = CredentialRef::new("account/integration-test/incoming");

    // Start from a known-clean state (idempotent delete).
    store.delete_secret(&reference).unwrap();
    assert_eq!(store.get_secret(&reference).unwrap(), None);

    store.set_secret(&reference, "integration-secret").unwrap();
    assert_eq!(
        store.get_secret(&reference).unwrap().as_deref(),
        Some("integration-secret")
    );

    // Clean up so the test leaves no residue in the keychain.
    store.delete_secret(&reference).unwrap();
    assert_eq!(store.get_secret(&reference).unwrap(), None);
}
