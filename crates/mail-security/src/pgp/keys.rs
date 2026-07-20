//! OpenPGP key generation, import/export, and metadata extraction.
//!
//! Keys are always handled in ASCII-armored form at this crate's boundary: a
//! generated or imported key is returned as armored text, and the non-secret
//! metadata ([`KeyMetadata`]) is derived from the parsed key so the storage
//! layer can index it without re-parsing. Secret key material never leaves this
//! module except as the armored blob the caller hands to the OS keychain
//! (ADR-0001); it is never written to the database.
//!
//! Generated keys are v4 Ed25519 (signing primary) + ECDH Curve25519
//! (encryption subkey): a modern, compact, widely interoperable default that,
//! for keys we create, avoids RSA entirely (and thus the RUSTSEC-2023-0071
//! concern documented in ADR-0015 for *received* RSA keys).

use pgp::composed::{
    ArmorOptions, Deserializable, EncryptionCaps, KeyType, SecretKeyParamsBuilder, SignedPublicKey,
    SignedSecretKey, SubkeyParamsBuilder,
};
use pgp::crypto::ecc_curve::ECCCurve;
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::types::KeyDetails;

use super::{crypto_err, invalid_key};
use axomail_core::MailError;

/// Non-secret, indexable description of an OpenPGP key.
///
/// Every field is derived from the public portion of the key, so it is safe to
/// store in the database and cross the IPC boundary. The armored key bytes are
/// carried separately by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMetadata {
    /// Full key fingerprint, lowercase hex without separators.
    pub fingerprint: String,
    /// Long key id (the fingerprint's low 64 bits for v4), lowercase hex.
    pub key_id: String,
    /// User ids on the key (`Name <email>` strings), in key order.
    pub user_ids: Vec<String>,
    /// Public-key algorithm of the primary key (e.g. `EdDSALegacy`).
    pub algorithm: String,
    /// Key creation time as a Unix timestamp (seconds).
    pub created_at_unix: i64,
    /// Whether this description came from a secret key (vs. a public key).
    pub has_secret: bool,
    /// Whether the secret key material is locked with a passphrase. Always
    /// `false` for a public-only key.
    pub is_passphrase_protected: bool,
}

/// A freshly generated OpenPGP keypair, ready to persist.
///
/// `secret_armored` goes to the OS keychain; `public_armored` and `metadata`
/// go to the database.
#[derive(Debug, Clone)]
pub struct GeneratedKey {
    /// ASCII-armored secret key (keychain-bound; never stored in the DB).
    pub secret_armored: String,
    /// ASCII-armored public key (safe to store and share).
    pub public_armored: String,
    /// Non-secret metadata derived from the key.
    pub metadata: KeyMetadata,
}

/// Generates a new Ed25519 / Curve25519 OpenPGP keypair for `user_id`.
///
/// `user_id` is the OpenPGP identity string, conventionally `Name <email>` (or
/// just the bare address). When `passphrase` is `Some`, both the primary and
/// the encryption subkey are locked with it, so decrypting or signing later
/// requires the same passphrase; the passphrase itself is never retained by
/// this crate.
///
/// This is CPU-bound and does not perform any I/O; callers on an async runtime
/// should run it on a blocking-safe context.
///
/// # Errors
///
/// Returns [`MailError::Crypto`] if key generation or armoring fails, or
/// [`MailError::Configuration`] if `user_id` is empty.
pub fn generate_keypair(
    user_id: &str,
    passphrase: Option<&str>,
) -> Result<GeneratedKey, MailError> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err(MailError::Configuration {
            message: "a PGP key needs a non-empty user id".to_owned(),
        });
    }

    let mut subkey = SubkeyParamsBuilder::default();
    subkey
        .key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
        .can_encrypt(EncryptionCaps::All);
    if let Some(pw) = passphrase {
        subkey.passphrase(Some(pw.to_owned()));
    }
    let subkey = subkey.build().map_err(|_| crypto_err("build PGP subkey"))?;

    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(KeyType::Ed25519Legacy)
        .can_certify(true)
        .can_sign(true)
        .primary_user_id(user_id.to_owned())
        .preferred_symmetric_algorithms(
            vec![SymmetricKeyAlgorithm::AES256, SymmetricKeyAlgorithm::AES128].into(),
        )
        .preferred_hash_algorithms(vec![HashAlgorithm::Sha256].into())
        .subkeys(vec![subkey]);
    if let Some(pw) = passphrase {
        params.passphrase(Some(pw.to_owned()));
    }
    let params = params
        .build()
        .map_err(|_| crypto_err("build PGP key params"))?;

    let mut rng = rand::thread_rng();
    let secret = params
        .generate(&mut rng)
        .map_err(|_| crypto_err("generate PGP key"))?;
    let public = secret.to_public_key();

    let secret_armored = secret
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("armor secret key"))?;
    let public_armored = public
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("armor public key"))?;
    let metadata = secret_metadata(&secret);

    Ok(GeneratedKey {
        secret_armored,
        public_armored,
        metadata,
    })
}

/// Parses an ASCII-armored public key, returning it re-armored (canonical) with
/// its metadata. Rejects input that is not a single OpenPGP public key.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the armor is missing or the block is
/// not a public key.
pub fn import_public_key(armored: &str) -> Result<(String, KeyMetadata), MailError> {
    let (key, _headers) =
        SignedPublicKey::from_string(armored).map_err(|_| invalid_key("public key"))?;
    let metadata = public_metadata(&key);
    let canonical = key
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("re-armor public key"))?;
    Ok((canonical, metadata))
}

/// Parses an ASCII-armored secret key, returning it re-armored with its
/// metadata (which records whether it is passphrase-locked). The secret bytes
/// are only ever handed back to the caller for keychain storage.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the armor is missing or the block is
/// not a secret key.
pub fn import_secret_key(armored: &str) -> Result<(String, KeyMetadata), MailError> {
    let (key, _headers) =
        SignedSecretKey::from_string(armored).map_err(|_| invalid_key("secret key"))?;
    let metadata = secret_metadata(&key);
    let canonical = key
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("re-armor secret key"))?;
    Ok((canonical, metadata))
}

/// Derives the armored public key and metadata from an armored secret key, so a
/// keychain-held secret key can publish its public half without re-entering it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the secret armor cannot be parsed.
pub fn public_from_secret(secret_armored: &str) -> Result<(String, KeyMetadata), MailError> {
    let (secret, _headers) =
        SignedSecretKey::from_string(secret_armored).map_err(|_| invalid_key("secret key"))?;
    let public = secret.to_public_key();
    let metadata = public_metadata(&public);
    let armored = public
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("armor public key"))?;
    Ok((armored, metadata))
}

/// Builds [`KeyMetadata`] from a parsed public key.
fn public_metadata(key: &SignedPublicKey) -> KeyMetadata {
    KeyMetadata {
        fingerprint: format!("{:x}", key.fingerprint()),
        key_id: hex_lower(key.legacy_key_id().as_ref()),
        user_ids: user_ids(&key.details.users),
        algorithm: format!("{:?}", key.algorithm()),
        created_at_unix: i64::from(key.created_at().as_secs()),
        has_secret: false,
        is_passphrase_protected: false,
    }
}

/// Builds [`KeyMetadata`] from a parsed secret key.
fn secret_metadata(key: &SignedSecretKey) -> KeyMetadata {
    KeyMetadata {
        fingerprint: format!("{:x}", key.primary_key.fingerprint()),
        key_id: hex_lower(key.primary_key.legacy_key_id().as_ref()),
        user_ids: user_ids(&key.details.users),
        algorithm: format!("{:?}", key.primary_key.algorithm()),
        created_at_unix: i64::from(key.primary_key.created_at().as_secs()),
        has_secret: true,
        is_passphrase_protected: key.primary_key.secret_params().is_encrypted(),
    }
}

/// Extracts the UTF-8 user-id strings from a key's signed users, skipping any
/// non-UTF-8 identity.
fn user_ids(users: &[pgp::types::SignedUser]) -> Vec<String> {
    users
        .iter()
        .filter_map(|user| std::str::from_utf8(user.id.id()).ok())
        .map(str::to_owned)
        .collect()
}

/// Lowercase hex-encodes a byte slice without separators.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn generate_produces_armored_keypair_and_metadata() {
        let key = generate_keypair("Alice <alice@example.org>", None).unwrap();
        assert!(key.secret_armored.contains("BEGIN PGP PRIVATE KEY BLOCK"));
        assert!(key.public_armored.contains("BEGIN PGP PUBLIC KEY BLOCK"));
        assert_eq!(key.metadata.fingerprint.len(), 40); // v4 fingerprint = 20 bytes
        assert!(
            key.metadata
                .fingerprint
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
        assert_eq!(
            key.metadata.user_ids,
            vec!["Alice <alice@example.org>".to_owned()]
        );
        assert!(key.metadata.has_secret);
        assert!(!key.metadata.is_passphrase_protected);
    }

    #[test]
    fn empty_user_id_is_rejected() {
        let err = generate_keypair("   ", None).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }

    #[test]
    fn public_import_round_trips_and_matches_fingerprint() {
        let generated = generate_keypair("Bob <bob@example.org>", None).unwrap();
        let (armored, meta) = import_public_key(&generated.public_armored).unwrap();
        assert!(armored.contains("BEGIN PGP PUBLIC KEY BLOCK"));
        assert_eq!(meta.fingerprint, generated.metadata.fingerprint);
        assert_eq!(meta.key_id, generated.metadata.key_id);
        assert!(!meta.has_secret);
    }

    #[test]
    fn public_from_secret_matches_public_export() {
        let generated = generate_keypair("Carol <carol@example.org>", None).unwrap();
        let (public_armored, meta) = public_from_secret(&generated.secret_armored).unwrap();
        assert_eq!(meta.fingerprint, generated.metadata.fingerprint);
        let (_, reimport) = import_public_key(&public_armored).unwrap();
        assert_eq!(reimport.fingerprint, generated.metadata.fingerprint);
    }

    #[test]
    fn passphrase_protected_key_reports_locked_metadata() {
        let key = generate_keypair("Dan <dan@example.org>", Some("s3cret")).unwrap();
        assert!(key.metadata.is_passphrase_protected);
        // The secret armor round-trips and still reports as locked.
        let (_, meta) = import_secret_key(&key.secret_armored).unwrap();
        assert!(meta.is_passphrase_protected);
    }

    #[test]
    fn importing_garbage_is_a_configuration_error() {
        let err = import_public_key("not a key").unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
        let err = import_secret_key("not a key").unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }

    #[test]
    fn public_import_rejects_a_secret_key_block() {
        let generated = generate_keypair("Eve <eve@example.org>", None).unwrap();
        // Feeding a secret key into the public importer must fail.
        let err = import_public_key(&generated.secret_armored).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }
}
