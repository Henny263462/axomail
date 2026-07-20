//! Signature-to-sender matching helpers for [`verify_detached`](super::verify_detached).
//!
//! These decide *which* held key (primary or a signing subkey) a detached
//! signature was made by, and whether that key's identity matches the message's
//! sender — the checks that turn a bare cryptographic verification into a
//! sender-bound result. They are byte/logic-only and consult no I/O.

use pgp::composed::{DetachedSignature, SignedPublicKey};
use pgp::types::KeyDetails;

/// Whether `detached` verifies under `key`'s primary key or any of its
/// signing-capable subkeys whose key id matches the signature's issuer.
///
/// Enumerating subkeys is essential: GnuPG and Thunderbird sign with a
/// dedicated signing **subkey** by default, so the issuer id names the subkey,
/// not the primary — matching only the primary would fail every such message.
pub(super) fn signature_verifies_under(
    detached: &DetachedSignature,
    key: &SignedPublicKey,
    issuer_ids: &[String],
    content: &[u8],
) -> bool {
    // An issuer-less signature (rare) is tried against the primary only.
    let primary_id = hex_lower(key.legacy_key_id().as_ref());
    if id_is_named(issuer_ids, &primary_id) && detached.verify(key, content).is_ok() {
        return true;
    }
    key.public_subkeys.iter().any(|subkey| {
        subkey.algorithm().can_sign()
            && id_is_named(issuer_ids, &hex_lower(subkey.legacy_key_id().as_ref()))
            && detached.verify(subkey, content).is_ok()
    })
}

/// Whether the signature's issuer id names this cert's primary key or one of its
/// signing-capable subkeys (regardless of whether verification then succeeds).
pub(super) fn issuer_matches_cert(key: &SignedPublicKey, issuer_ids: &[String]) -> bool {
    if id_is_named(issuer_ids, &hex_lower(key.legacy_key_id().as_ref())) {
        return true;
    }
    key.public_subkeys.iter().any(|subkey| {
        subkey.algorithm().can_sign()
            && id_is_named(issuer_ids, &hex_lower(subkey.legacy_key_id().as_ref()))
    })
}

/// Whether `id` is named by the issuer id list. An empty list (a signature with
/// no issuer subpacket) is treated as naming every key, so it is still tried.
fn id_is_named(issuer_ids: &[String], id: &str) -> bool {
    issuer_ids.is_empty() || issuer_ids.iter().any(|issuer| issuer == id)
}

/// Whether any of the cert's User-IDs carries an email equal to `sender`
/// (already normalized, lowercase).
pub(super) fn signer_matches_sender(key: &SignedPublicKey, sender: &str) -> bool {
    if sender.is_empty() {
        return false;
    }
    key.details.users.iter().any(|user| {
        std::str::from_utf8(user.id.id())
            .ok()
            .and_then(email_of_uid)
            .is_some_and(|email| email == sender)
    })
}

/// The cert's first (primary) User-ID as a UTF-8 string, if any.
pub(super) fn primary_user_id(key: &SignedPublicKey) -> Option<String> {
    key.details
        .users
        .iter()
        .find_map(|user| std::str::from_utf8(user.id.id()).ok())
        .map(str::to_owned)
}

/// Extracts the normalized email from an OpenPGP User-ID string.
///
/// A User-ID is conventionally `Name <email>`; the bracketed address is taken
/// when present, otherwise the whole trimmed string is treated as a bare email.
fn email_of_uid(uid: &str) -> Option<String> {
    let email = uid
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.split_once('>').map(|(email, _)| email))
        .unwrap_or(uid);
    let email = email.trim().to_lowercase();
    (!email.is_empty()).then_some(email)
}

/// Normalizes a `From`/address string to its lowercase email for comparison,
/// mirroring [`email_of_uid`] so a bare address or a `Name <email>` both work.
pub(super) fn normalize_email(address: &str) -> String {
    email_of_uid(address).unwrap_or_default()
}

/// Lowercase hex-encodes a byte slice without separators.
pub(super) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::email_of_uid;

    #[test]
    fn email_of_uid_extracts_bracketed_and_bare_addresses() {
        assert_eq!(
            email_of_uid("Alice <Alice@Example.ORG>").as_deref(),
            Some("alice@example.org")
        );
        assert_eq!(
            email_of_uid("bob@example.org").as_deref(),
            Some("bob@example.org")
        );
        assert_eq!(email_of_uid("   ").as_deref(), None);
    }
}
