//! Detached OpenPGP signatures: create one over a byte range, and verify one
//! against a set of candidate signer public keys — **binding the result to the
//! message's sender identity**.
//!
//! Detached signatures are the form RFC 3156 PGP/MIME uses for
//! `multipart/signed`: the signature lives in a separate
//! `application/pgp-signature` part and covers the (CRLF-canonical) bytes of the
//! signed MIME entity. The same primitives verify an inline OpenPGP signature
//! once its covered bytes have been isolated.
//!
//! A cryptographically valid signature is **not** enough to show a green badge:
//! any correspondent whose public key we hold could otherwise sign a message,
//! forge a `From:` header naming someone else, and have it reported "valid".
//! Verification here therefore also checks that the signing key carries a
//! User-ID whose email matches the message's `From` address; only then is the
//! result [`Valid`](SignatureVerification::Valid). A signature that verifies
//! under a key whose identity does *not* match the sender is reported as the
//! distinct [`SignerMismatch`](SignatureVerification::SignerMismatch) — never a
//! plain "valid".

use pgp::composed::{ArmorOptions, Deserializable, DetachedSignature, SignedPublicKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::types::Password;

use super::sign_verify::{
    hex_lower, issuer_matches_cert, normalize_email, primary_user_id, signature_verifies_under,
    signer_matches_sender,
};
use super::{crypto_err, invalid_key};
use axomail_core::MailError;

/// The outcome of verifying a signature against a set of candidate keys.
///
/// This is a *result*, not an error: a tampered message legitimately verifies
/// as [`Invalid`](Self::Invalid), a message signed by a key we do not hold is
/// [`UnknownKey`](Self::UnknownKey), and a message signed by a key whose
/// identity does not match the sender is [`SignerMismatch`](Self::SignerMismatch).
/// Only an internal fault (unparseable input) is reported as a [`MailError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureVerification {
    /// The signature is cryptographically valid **and** the signing key carries
    /// a User-ID whose email matches the message's `From` address.
    Valid,
    /// The signature is cryptographically valid, but the signing key's
    /// User-IDs do **not** include the sender's `From` address — the signer is
    /// not (provably) the claimed sender. Must never be shown as plain "valid".
    SignerMismatch,
    /// A candidate key matched the signer, but verification failed — the
    /// content was altered or the signature is corrupt.
    Invalid,
    /// None of the supplied keys is the signer, so validity cannot be judged.
    UnknownKey,
}

/// A verification result plus the identity of the key that produced (or would
/// have produced) it.
///
/// `signer` is the matched key's primary User-ID string (e.g.
/// `"Mallory <mallory@example.org>"`), populated whenever a candidate key
/// matched the signature's issuer — for [`Valid`](SignatureVerification::Valid),
/// [`SignerMismatch`](SignatureVerification::SignerMismatch), and
/// [`Invalid`](SignatureVerification::Invalid). It is `None` only for
/// [`UnknownKey`](SignatureVerification::UnknownKey). The reading pane surfaces
/// it so the badge can name *who* signed, which is what makes a `SignerMismatch`
/// legible to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSignature {
    /// The verification status.
    pub verification: SignatureVerification,
    /// The matched key's primary User-ID, when a candidate matched the issuer.
    pub signer: Option<String>,
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
/// candidate signer public keys in `candidate_public_armored`, binding the
/// result to the message's `from_email` sender address.
///
/// The signature's issuer key id selects which candidate (its primary key or any
/// of its signing-capable subkeys) is the purported signer. If one matches and
/// the signature verifies, the result is bound to the sender:
///
/// - [`Valid`](SignatureVerification::Valid) when the matched key carries a
///   User-ID whose email equals `from_email` (case-insensitive);
/// - [`SignerMismatch`](SignatureVerification::SignerMismatch) when it verifies
///   but no User-ID matches `from_email` — the signer is not the claimed sender.
///
/// If a candidate matched the issuer but verification failed, the result is
/// [`Invalid`](SignatureVerification::Invalid); if none matched, it is
/// [`UnknownKey`](SignatureVerification::UnknownKey). Candidate keys that fail
/// to parse are skipped. In every case the matched key's User-ID (if any) is
/// returned as the signer identity.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the signature armor itself cannot be
/// parsed.
pub fn verify_detached(
    candidate_public_armored: &[String],
    signature: &str,
    content: &[u8],
    from_email: &str,
) -> Result<VerifiedSignature, MailError> {
    let (detached, _headers) =
        DetachedSignature::from_string(signature).map_err(|_| invalid_key("signature"))?;

    let issuer_ids: Vec<String> = detached
        .signature
        .issuer_key_id()
        .into_iter()
        .map(|id| hex_lower(id.as_ref()))
        .collect();

    let sender = normalize_email(from_email);

    // Remember a candidate that matched the issuer but failed to verify, so an
    // altered message (Invalid) is told apart from one we lack the key for
    // (UnknownKey) — while still naming the purported signer.
    let mut matched_but_invalid: Option<String> = None;

    for armored in candidate_public_armored {
        let Ok((key, _)) = SignedPublicKey::from_string(armored) else {
            continue;
        };
        let uid = primary_user_id(&key);

        // Does the signature verify under this cert's primary key or one of its
        // signing-capable subkeys whose id matches the issuer?
        if signature_verifies_under(&detached, &key, &issuer_ids, content) {
            let verification = if signer_matches_sender(&key, &sender) {
                SignatureVerification::Valid
            } else {
                SignatureVerification::SignerMismatch
            };
            return Ok(VerifiedSignature {
                verification,
                signer: uid,
            });
        }

        // The issuer named this cert (or a subkey), but nothing verified: the
        // message was altered or the signature is corrupt.
        if issuer_matches_cert(&key, &issuer_ids) {
            matched_but_invalid.get_or_insert(uid.unwrap_or_default());
        }
    }

    Ok(match matched_but_invalid {
        Some(signer) => VerifiedSignature {
            verification: SignatureVerification::Invalid,
            signer: (!signer.is_empty()).then_some(signer),
        },
        None => VerifiedSignature {
            verification: SignatureVerification::UnknownKey,
            signer: None,
        },
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pgp::generate_keypair;

    const MESSAGE: &[u8] = b"Content-Type: text/plain\r\n\r\nThe treaty is signed.\r\n";
    const FROM: &str = "s@example.org";

    #[test]
    fn valid_signature_bound_to_matching_sender() {
        let key = generate_keypair("Signer <s@example.org>", None).unwrap();
        let sig = sign_detached(&key.secret_armored, None, MESSAGE).unwrap();
        let out = verify_detached(&[key.public_armored], &sig, MESSAGE, FROM).unwrap();
        assert_eq!(out.verification, SignatureVerification::Valid);
        assert_eq!(out.signer.as_deref(), Some("Signer <s@example.org>"));
    }

    #[test]
    fn valid_under_a_held_key_but_sender_mismatch_is_not_valid() {
        // SECURITY: a correspondent whose key we hold signs, but the message's
        // From claims someone else. The signature verifies cryptographically,
        // yet must NOT be reported valid — it is a signer/sender mismatch, and
        // the true signer is named.
        let mallory = generate_keypair("Mallory <mallory@evil.example>", None).unwrap();
        let sig = sign_detached(&mallory.secret_armored, None, MESSAGE).unwrap();
        let out = verify_detached(
            &[mallory.public_armored],
            &sig,
            MESSAGE,
            "cfo@company.example",
        )
        .unwrap();
        assert_eq!(out.verification, SignatureVerification::SignerMismatch);
        assert_ne!(out.verification, SignatureVerification::Valid);
        assert_eq!(
            out.signer.as_deref(),
            Some("Mallory <mallory@evil.example>")
        );
    }

    #[test]
    fn sender_match_is_case_insensitive() {
        let key = generate_keypair("Signer <S@Example.ORG>", None).unwrap();
        let sig = sign_detached(&key.secret_armored, None, MESSAGE).unwrap();
        let out = verify_detached(&[key.public_armored], &sig, MESSAGE, "s@example.org").unwrap();
        assert_eq!(out.verification, SignatureVerification::Valid);
    }

    #[test]
    fn tampered_content_is_invalid() {
        let key = generate_keypair("Signer <s@example.org>", None).unwrap();
        let sig = sign_detached(&key.secret_armored, None, MESSAGE).unwrap();
        let tampered = b"Content-Type: text/plain\r\n\r\nThe treaty is broken.\r\n";
        let out = verify_detached(&[key.public_armored], &sig, tampered, FROM).unwrap();
        assert_eq!(out.verification, SignatureVerification::Invalid);
    }

    #[test]
    fn signature_from_an_unheld_key_is_unknown() {
        let signer = generate_keypair("Signer <s@example.org>", None).unwrap();
        let stranger = generate_keypair("Other <o@example.org>", None).unwrap();
        let sig = sign_detached(&signer.secret_armored, None, MESSAGE).unwrap();
        // We only hold the stranger's key, not the signer's.
        let out = verify_detached(&[stranger.public_armored], &sig, MESSAGE, FROM).unwrap();
        assert_eq!(out.verification, SignatureVerification::UnknownKey);
        assert!(out.signer.is_none());
    }

    #[test]
    fn passphrase_protected_key_signs_and_verifies() {
        let key = generate_keypair("Locked <l@example.org>", Some("pw")).unwrap();
        let sig = sign_detached(&key.secret_armored, Some("pw"), MESSAGE).unwrap();
        let out = verify_detached(&[key.public_armored], &sig, MESSAGE, "l@example.org").unwrap();
        assert_eq!(out.verification, SignatureVerification::Valid);
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
        let err =
            verify_detached(&[key.public_armored], "not a signature", MESSAGE, FROM).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }

    /// Builds a cert whose signing is done by a dedicated signing **subkey**
    /// (GnuPG/Thunderbird's default layout), returning the armored public key
    /// and an armored detached signature over `MESSAGE` made by that subkey.
    fn signed_by_subkey(uid: &str) -> (String, String) {
        use pgp::composed::{
            ArmorOptions, EncryptionCaps, KeyType, SecretKeyParamsBuilder, SubkeyParamsBuilder,
        };
        use pgp::crypto::ecc_curve::ECCCurve;
        use pgp::types::KeyDetails as _;

        let mut signing_subkey = SubkeyParamsBuilder::default();
        signing_subkey
            .key_type(KeyType::Ed25519Legacy)
            .can_sign(true);
        let signing_subkey = signing_subkey.build().unwrap();

        let mut enc_subkey = SubkeyParamsBuilder::default();
        enc_subkey
            .key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
            .can_encrypt(EncryptionCaps::All);
        let enc_subkey = enc_subkey.build().unwrap();

        let mut params = SecretKeyParamsBuilder::default();
        params
            .key_type(KeyType::Ed25519Legacy)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id(uid.to_owned())
            .preferred_hash_algorithms(vec![HashAlgorithm::Sha256].into())
            .subkeys(vec![signing_subkey, enc_subkey]);
        let params = params.build().unwrap();

        let mut rng = rand::thread_rng();
        let secret = params.generate(&mut rng).unwrap();
        let public_armored = secret
            .to_public_key()
            .to_armored_string(ArmorOptions::default())
            .unwrap();

        // Sign with the signing *subkey*, so the signature's issuer id names the
        // subkey — not the primary. Verification must enumerate subkeys to match.
        let subkey = secret
            .secret_subkeys
            .iter()
            .find(|sub| sub.key.algorithm().can_sign())
            .expect("a signing subkey");
        let sig = DetachedSignature::sign_binary_data(
            &mut rng,
            &subkey.key,
            &Password::empty(),
            HashAlgorithm::Sha256,
            MESSAGE,
        )
        .unwrap();
        let sig_armored = sig.to_armored_string(ArmorOptions::default()).unwrap();
        (public_armored, sig_armored)
    }

    #[test]
    fn signature_by_a_signing_subkey_verifies() {
        // SECURITY/interop: GnuPG and Thunderbird sign with a signing subkey by
        // default. Matching only the primary key id would report every such
        // message as UnknownKey; enumerating signing subkeys makes it Valid.
        let (public_armored, sig) = signed_by_subkey("Subkey Signer <sub@example.org>");
        let out = verify_detached(&[public_armored], &sig, MESSAGE, "sub@example.org").unwrap();
        assert_eq!(out.verification, SignatureVerification::Valid);
        assert_eq!(
            out.signer.as_deref(),
            Some("Subkey Signer <sub@example.org>")
        );
    }
}
