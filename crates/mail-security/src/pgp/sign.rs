//! Detached OpenPGP signatures: create one over a byte range, and verify one
//! against a set of candidate signer public keys.
//!
//! Detached signatures are the form RFC 3156 PGP/MIME uses for
//! `multipart/signed`: the signature lives in a separate
//! `application/pgp-signature` part and covers the (CRLF-canonical) bytes of the
//! signed MIME entity. The same primitives verify an inline OpenPGP signature
//! once its covered bytes have been isolated.

use pgp::composed::{ArmorOptions, Deserializable, DetachedSignature, SignedPublicKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::types::{KeyDetails, Password};

use super::{crypto_err, invalid_key};
use axomail_core::MailError;

/// The outcome of verifying a signature.
///
/// This is a *result*, not an error: a tampered message legitimately verifies
/// as [`Invalid`](Self::Invalid), and a message signed by a key we do not hold
/// is [`UnknownKey`](Self::UnknownKey). Only an internal fault (unparseable
/// input) is reported as a [`MailError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureVerification {
    /// The signature is cryptographically valid for one of the candidate keys.
    Valid,
    /// A candidate key matched the signer, but verification failed — the
    /// content was altered or the signature is corrupt.
    Invalid,
    /// None of the supplied keys is the signer, so validity cannot be judged.
    UnknownKey,
}

/// Creates a detached binary signature over `content` with the secret key in
/// `secret_armored`, returned ASCII-armored.
///
/// `passphrase` unlocks a passphrase-protected key; pass `None` for an
/// unprotected key. The signature is computed with SHA-256 over exactly the
/// bytes given, so the caller must present the same CRLF-canonical bytes it
/// places in the `multipart/signed` entity.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the secret key cannot be parsed and
/// [`MailError::Crypto`] if signing fails (for example a wrong passphrase).
pub fn sign_detached(
    secret_armored: &str,
    passphrase: Option<&str>,
    content: &[u8],
) -> Result<String, MailError> {
    let (secret, _headers) = pgp::composed::SignedSecretKey::from_string(secret_armored)
        .map_err(|_| invalid_key("secret key"))?;
    let password = passphrase.map_or_else(Password::empty, Password::from);
    let mut rng = rand::thread_rng();
    let signature = DetachedSignature::sign_binary_data(
        &mut rng,
        &secret.primary_key,
        &password,
        HashAlgorithm::Sha256,
        content,
    )
    .map_err(|_| crypto_err("sign the message"))?;
    signature
        .to_armored_string(ArmorOptions::default())
        .map_err(|_| crypto_err("armor the signature"))
}

/// Verifies an armored detached `signature` over `content` against the
/// candidate signer public keys in `candidate_public_armored`.
///
/// The signature's issuer key id selects which candidate is the purported
/// signer: if one matches, the signature is checked against it and the result
/// is [`Valid`](SignatureVerification::Valid) or
/// [`Invalid`](SignatureVerification::Invalid); if none matches, the result is
/// [`UnknownKey`](SignatureVerification::UnknownKey). Candidate keys that fail
/// to parse are skipped.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the signature armor itself cannot be
/// parsed.
pub fn verify_detached(
    candidate_public_armored: &[String],
    signature: &str,
    content: &[u8],
) -> Result<SignatureVerification, MailError> {
    let (detached, _headers) =
        DetachedSignature::from_string(signature).map_err(|_| invalid_key("signature"))?;

    let issuer_ids: Vec<String> = detached
        .signature
        .issuer_key_id()
        .into_iter()
        .map(|id| hex_lower(id.as_ref()))
        .collect();

    // Track whether any candidate was even a plausible signer, to tell an
    // altered message (Invalid) apart from one we simply lack the key for
    // (UnknownKey).
    let mut saw_candidate_signer = false;
    for armored in candidate_public_armored {
        let Ok((key, _)) = SignedPublicKey::from_string(armored) else {
            continue;
        };
        let key_id = hex_lower(key.legacy_key_id().as_ref());
        let is_named_signer = issuer_ids.is_empty() || issuer_ids.iter().any(|id| id == &key_id);
        if !is_named_signer {
            continue;
        }
        saw_candidate_signer = true;
        if detached.verify(&key, content).is_ok() {
            return Ok(SignatureVerification::Valid);
        }
    }

    Ok(if saw_candidate_signer {
        SignatureVerification::Invalid
    } else {
        SignatureVerification::UnknownKey
    })
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
    use crate::pgp::generate_keypair;

    const MESSAGE: &[u8] = b"Content-Type: text/plain\r\n\r\nThe treaty is signed.\r\n";

    #[test]
    fn valid_signature_verifies() {
        let key = generate_keypair("Signer <s@example.org>", None).unwrap();
        let sig = sign_detached(&key.secret_armored, None, MESSAGE).unwrap();
        let status = verify_detached(&[key.public_armored], &sig, MESSAGE).unwrap();
        assert_eq!(status, SignatureVerification::Valid);
    }

    #[test]
    fn tampered_content_is_invalid() {
        let key = generate_keypair("Signer <s@example.org>", None).unwrap();
        let sig = sign_detached(&key.secret_armored, None, MESSAGE).unwrap();
        let tampered = b"Content-Type: text/plain\r\n\r\nThe treaty is broken.\r\n";
        let status = verify_detached(&[key.public_armored], &sig, tampered).unwrap();
        assert_eq!(status, SignatureVerification::Invalid);
    }

    #[test]
    fn signature_from_an_unheld_key_is_unknown() {
        let signer = generate_keypair("Signer <s@example.org>", None).unwrap();
        let stranger = generate_keypair("Other <o@example.org>", None).unwrap();
        let sig = sign_detached(&signer.secret_armored, None, MESSAGE).unwrap();
        // We only hold the stranger's key, not the signer's.
        let status = verify_detached(&[stranger.public_armored], &sig, MESSAGE).unwrap();
        assert_eq!(status, SignatureVerification::UnknownKey);
    }

    #[test]
    fn passphrase_protected_key_signs_and_verifies() {
        let key = generate_keypair("Locked <l@example.org>", Some("pw")).unwrap();
        let sig = sign_detached(&key.secret_armored, Some("pw"), MESSAGE).unwrap();
        let status = verify_detached(&[key.public_armored], &sig, MESSAGE).unwrap();
        assert_eq!(status, SignatureVerification::Valid);
    }

    #[test]
    fn wrong_passphrase_fails_to_sign() {
        let key = generate_keypair("Locked <l@example.org>", Some("pw")).unwrap();
        let err = sign_detached(&key.secret_armored, Some("nope"), MESSAGE).unwrap_err();
        assert!(matches!(err, MailError::Crypto { .. }));
    }

    #[test]
    fn malformed_signature_is_a_configuration_error() {
        let key = generate_keypair("Signer <s@example.org>", None).unwrap();
        let err = verify_detached(&[key.public_armored], "not a signature", MESSAGE).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }
}
