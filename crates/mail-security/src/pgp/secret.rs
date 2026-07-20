//! Keychain reference scheme for OpenPGP secret keys.
//!
//! A secret key's armored bytes are themselves the secret, so — like passwords
//! and OAuth tokens (ADR-0001) — they live only in the OS keychain via
//! [`SecretStore`](crate::SecretStore), referenced by a non-secret
//! [`CredentialRef`]. Only the public key and metadata are ever persisted in
//! the database.

use axomail_core::CredentialRef;

/// Builds the keychain reference for a PGP secret key, scoped by fingerprint.
///
/// The scheme is `pgp/key/<fingerprint>` (lowercase-hex fingerprint), for
/// example `pgp/key/a1b2…`. Like [`credential_ref_for_account`] it is a stable
/// on-disk contract: the same key must always map to the same reference so the
/// stored secret is found again across restarts. It is disjoint from the
/// `account/…` namespace so PGP keys and account credentials never collide.
///
/// [`credential_ref_for_account`]: crate::credential_ref_for_account
#[must_use]
pub fn credential_ref_for_pgp(fingerprint: &str) -> CredentialRef {
    CredentialRef::new(format!("pgp/key/{fingerprint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_is_namespaced_and_stable() {
        let reference = credential_ref_for_pgp("a1b2c3");
        assert_eq!(reference.as_str(), "pgp/key/a1b2c3");
        // Disjoint from the account credential namespace.
        assert!(reference.as_str().starts_with("pgp/"));
    }
}
