//! Choosing a **valid** encryption key for a recipient.
//!
//! A recipient is encrypted to the first encryption (sub)key that is actually
//! usable right now: encryption-capable by algorithm, not revoked, and not
//! expired. A key whose only encryption subkey is revoked or expired is rejected
//! rather than used — a message must never be encrypted to a key that cannot (or
//! should not) open it. The same predicate backs the composer's `canEncrypt`
//! gate so "Encrypt" is not offered for a send that would then fail.

use std::time::{SystemTime, UNIX_EPOCH};

use pgp::composed::{Deserializable, SignedPublicKey, SignedPublicSubKey};
use pgp::packet::SignatureType;
use pgp::types::KeyDetails;

use axomail_core::MailError;

/// A recipient key selected for encryption: either a subkey or, for a key whose
/// primary is itself an encryption key (e.g. a single RSA key), the primary.
pub(super) enum EncryptionTarget<'a> {
    /// An encryption-capable subkey.
    Subkey(&'a SignedPublicSubKey),
    /// The primary key, used only when it is itself encryption-capable.
    Primary(&'a SignedPublicKey),
}

/// Whether an armored public key currently has a usable encryption (sub)key: an
/// encryption-capable, non-revoked, non-expired (sub)key exists right now.
///
/// The composer uses this to decide whether "Encrypt" is offered for a
/// recipient — offering it for a key whose only encryption subkey is expired or
/// revoked would let the user pick Encrypt only for the send to then fail.
#[must_use]
pub fn public_key_can_encrypt(public_armored: &str) -> bool {
    match SignedPublicKey::from_string(public_armored) {
        Ok((key, _)) => select_encryption_target(&key, now_unix_secs()).is_ok(),
        Err(_) => false,
    }
}

/// Picks a valid encryption target for a recipient key, preferring a valid
/// encryption **subkey** (the ECDH subkey of a modern Ed25519 cert) and falling
/// back to the primary only when the primary is itself an encryption key.
///
/// Revoked and expired encryption subkeys are skipped. If no valid encryption
/// (sub)key exists the send is failed rather than producing an unreadable or
/// insecure message.
pub(super) fn select_encryption_target(
    key: &SignedPublicKey,
    now_secs: u32,
) -> Result<EncryptionTarget<'_>, MailError> {
    if let Some(subkey) = key
        .public_subkeys
        .iter()
        .find(|subkey| encryption_subkey_is_usable(subkey, now_secs))
    {
        return Ok(EncryptionTarget::Subkey(subkey));
    }
    if key.algorithm().can_encrypt() {
        return Ok(EncryptionTarget::Primary(key));
    }
    Err(MailError::Configuration {
        message: "a recipient key has no valid (non-revoked, non-expired) encryption key"
            .to_owned(),
    })
}

/// Whether an encryption subkey may be used right now: it must use an
/// encryption-capable algorithm, not be revoked, and not have expired.
fn encryption_subkey_is_usable(subkey: &SignedPublicSubKey, now_secs: u32) -> bool {
    if !subkey.algorithm().can_encrypt() {
        return false;
    }
    let expires_at = subkey_expiry(subkey);
    encryption_key_usable(true, subkey_revoked(subkey), expires_at, now_secs)
}

/// Whether the subkey carries a revocation self-signature.
fn subkey_revoked(subkey: &SignedPublicSubKey) -> bool {
    subkey
        .signatures
        .iter()
        .any(|sig| sig.typ() == Some(SignatureType::SubkeyRevocation))
}

/// The subkey's absolute expiry instant in Unix seconds, or `None` when it never
/// expires. Taken from the latest binding signature's key-expiration subpacket
/// (which is a duration from the subkey's creation time).
fn subkey_expiry(subkey: &SignedPublicSubKey) -> Option<u32> {
    let created = subkey.created_at().as_secs();
    subkey
        .signatures
        .iter()
        .filter(|sig| sig.typ() == Some(SignatureType::SubkeyBinding))
        .max_by_key(|sig| sig.created().map_or(0, |ts| ts.as_secs()))
        .and_then(|binding| binding.key_expiration_time())
        .map(|dur| created.saturating_add(dur.as_secs()))
}

/// The pure usability decision for an encryption key, isolated so the security
/// rule — an algorithm that cannot encrypt, a revoked key, or an expired key is
/// unusable — is directly testable without an OpenPGP fixture.
fn encryption_key_usable(
    algorithm_can_encrypt: bool,
    revoked: bool,
    expires_at_secs: Option<u32>,
    now_secs: u32,
) -> bool {
    if !algorithm_can_encrypt || revoked {
        return false;
    }
    match expires_at_secs {
        Some(expiry) => now_secs < expiry,
        None => true,
    }
}

/// Current time in Unix seconds, saturating into `u32` (OpenPGP v4 timestamps
/// are 32-bit) — the reference instant for expiry checks.
pub(super) fn now_unix_secs() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pgp::{encrypt_message, generate_keypair};

    const PLAINTEXT: &[u8] = b"Content-Type: text/plain\r\n\r\nMeet at the bridge.\r\n";

    #[test]
    fn encryption_key_usable_rejects_revoked_and_expired() {
        // SECURITY (finding #7): the usability rule refuses a revoked or expired
        // encryption key, and an algorithm that cannot encrypt.
        assert!(encryption_key_usable(true, false, None, 1_000));
        assert!(encryption_key_usable(true, false, Some(2_000), 1_000));
        // Expired: now is at/after the expiry instant.
        assert!(!encryption_key_usable(true, false, Some(1_000), 1_000));
        assert!(!encryption_key_usable(true, false, Some(1_000), 2_000));
        // Revoked, regardless of expiry.
        assert!(!encryption_key_usable(true, true, None, 1_000));
        // Not an encryption algorithm.
        assert!(!encryption_key_usable(false, false, None, 1_000));
    }

    /// Builds a public cert whose only encryption subkey expired long ago: an
    /// ECDH subkey created in 2017 with a one-hour lifetime, so it is expired
    /// relative to any realistic present. The binding signature is re-issued
    /// (validly) by the primary key with a `KeyExpirationTime` subpacket.
    fn cert_with_expired_encryption_subkey() -> String {
        use pgp::composed::{
            ArmorOptions, EncryptionCaps, KeyType, SecretKeyParamsBuilder, SubkeyParamsBuilder,
        };
        use pgp::crypto::ecc_curve::ECCCurve;
        use pgp::packet::{SignatureConfig, Subpacket, SubpacketData};
        use pgp::types::{Duration as PgpDuration, Password, Timestamp};

        // 2017-07-14; well in the past relative to any present run.
        let created = Timestamp::from_secs(1_500_000_000);

        let mut enc = SubkeyParamsBuilder::default();
        enc.key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
            .can_encrypt(EncryptionCaps::All)
            .created_at(created);
        let enc = enc.build().unwrap();

        let mut params = SecretKeyParamsBuilder::default();
        params
            .key_type(KeyType::Ed25519Legacy)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id("Expired <exp@example.org>".to_owned())
            .subkeys(vec![enc]);
        let params = params.build().unwrap();

        let mut rng = rand::thread_rng();
        let mut secret = params.generate(&mut rng).unwrap();

        // Re-sign the subkey binding with a one-hour KeyExpirationTime; combined
        // with the 2017 subkey creation time this makes the subkey long expired.
        let subkey_pub = secret.secret_subkeys[0].key.public_key();
        let mut config =
            SignatureConfig::from_key(&mut rng, &secret.primary_key, SignatureType::SubkeyBinding)
                .unwrap();
        config.hashed_subpackets = vec![
            Subpacket::regular(SubpacketData::SignatureCreationTime(created)).unwrap(),
            Subpacket::regular(SubpacketData::IssuerFingerprint(
                secret.primary_key.fingerprint(),
            ))
            .unwrap(),
            Subpacket::regular(SubpacketData::KeyExpirationTime(PgpDuration::from_secs(
                3_600,
            )))
            .unwrap(),
        ];
        let binding = config
            .sign_subkey_binding(
                &secret.primary_key,
                &secret.primary_key.public_key(),
                &Password::empty(),
                &subkey_pub,
            )
            .unwrap();
        secret.secret_subkeys[0].signatures = vec![binding];

        secret
            .to_public_key()
            .to_armored_string(ArmorOptions::default())
            .unwrap()
    }

    #[test]
    fn expired_encryption_subkey_is_not_used() {
        // SECURITY (finding #7): a recipient whose only encryption subkey has
        // expired must not be encrypted to — the send fails rather than
        // producing a message no one intends to be readable under a dead key.
        let armored = cert_with_expired_encryption_subkey();
        let err = encrypt_message(&[armored], &[], PLAINTEXT).unwrap_err();
        assert!(
            matches!(err, MailError::Configuration { .. }),
            "expired-only recipient key must be rejected, got {err:?}"
        );
    }

    #[test]
    fn public_key_can_encrypt_reflects_usable_encryption_key() {
        // SECURITY (finding #8): the composer's canEncrypt gate is false for a
        // recipient whose only encryption subkey is expired, and true for a
        // healthy key — so Encrypt is never offered for a send that would fail.
        let healthy = generate_keypair("Good <good@example.org>", None).unwrap();
        assert!(public_key_can_encrypt(&healthy.public_armored));
        assert!(!public_key_can_encrypt(
            &cert_with_expired_encryption_subkey()
        ));
        assert!(!public_key_can_encrypt("not a key"));
    }
}
