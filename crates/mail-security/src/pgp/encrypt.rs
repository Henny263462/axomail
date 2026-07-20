//! OpenPGP message encryption and decryption over raw byte payloads.
//!
//! These operate on opaque bytes: the caller decides what the plaintext is. For
//! PGP/MIME (RFC 3156) the encrypted payload is the inner MIME entity — which,
//! for a signed-and-encrypted message, is itself a `multipart/signed` block, so
//! signature verification is a separate step performed on the decrypted bytes.
//!
//! # Recipient privacy
//!
//! Recipients come in two kinds. *Visible* recipients (To/Cc, and ourselves)
//! get an ordinary PKESK packet carrying the encryption (sub)key id. *Hidden*
//! recipients — the Bcc set — get a **wildcard-key-id** ("throw-keyid") PKESK,
//! so the on-the-wire ciphertext never reveals that a Bcc'd party is a
//! recipient. Every recipient can still decrypt; a hidden one simply trial-
//! decrypts against the anonymous PKESK.
//!
//! # Key selection
//!
//! Each recipient is encrypted to a **valid** encryption (sub)key: revoked or
//! expired encryption subkeys are skipped, and a recipient with no usable
//! encryption key fails the send rather than producing a message that key
//! cannot open.

use pgp::composed::{
    ArmorOptions, Deserializable, Message, MessageBuilder, SignedPublicKey, SignedSecretKey,
};
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::types::Password;

use super::enc_select::{EncryptionTarget, now_unix_secs, select_encryption_target};
use super::{crypto_err, invalid_key};
use axomail_core::MailError;

/// The plaintext recovered from an encrypted OpenPGP message.
#[derive(Debug, Clone)]
pub struct DecryptedMessage {
    /// Decrypted payload bytes (for PGP/MIME, the inner MIME entity).
    pub bytes: Vec<u8>,
}

/// Encrypts `content` to every visible and hidden recipient, returning an
/// ASCII-armored OpenPGP message.
///
/// `visible_recipient_public_armored` are encrypted to normally (a To/Cc
/// recipient, or ourselves, whose key id may appear in the message).
/// `hidden_recipient_public_armored` are the Bcc set: each is encrypted to with
/// a wildcard PKESK key id, so the message does not disclose that they are a
/// recipient. At least one recipient (visible or hidden) is required.
///
/// Encryption is public-key (SEIPD v1 / AES-256): each recipient's encryption
/// (sub)key wraps the session key, so any one recipient's secret key decrypts
/// it. A recipient whose only encryption (sub)key is revoked or expired is
/// rejected — a message is never encrypted to a key that cannot (or should not)
/// open it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the recipient list is empty, a key
/// cannot be parsed, or a recipient has no valid encryption key, and
/// [`MailError::Crypto`] if encryption or armoring fails.
pub fn encrypt_message(
    visible_recipient_public_armored: &[String],
    hidden_recipient_public_armored: &[String],
    content: &[u8],
) -> Result<String, MailError> {
    if visible_recipient_public_armored.is_empty() && hidden_recipient_public_armored.is_empty() {
        return Err(MailError::Configuration {
            message: "encryption needs at least one recipient public key".to_owned(),
        });
    }
    let visible = parse_recipients(visible_recipient_public_armored)?;
    let hidden = parse_recipients(hidden_recipient_public_armored)?;

    let now = now_unix_secs();
    let mut rng = rand::thread_rng();
    let mut builder = MessageBuilder::from_bytes("", content.to_vec())
        .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);

    for recipient in &visible {
        match select_encryption_target(recipient, now)? {
            EncryptionTarget::Subkey(subkey) => builder.encrypt_to_key(&mut rng, subkey),
            EncryptionTarget::Primary(primary) => builder.encrypt_to_key(&mut rng, primary),
        }
        .map_err(|_| crypto_err("encrypt to a recipient key"))?;
    }
    for recipient in &hidden {
        // Anonymous (wildcard key id) PKESK: the ciphertext does not reveal that
        // this Bcc'd recipient is a recipient at all.
        match select_encryption_target(recipient, now)? {
            EncryptionTarget::Subkey(subkey) => builder.encrypt_to_key_anonymous(&mut rng, subkey),
            EncryptionTarget::Primary(primary) => {
                builder.encrypt_to_key_anonymous(&mut rng, primary)
            }
        }
        .map_err(|_| crypto_err("encrypt to a recipient key"))?;
    }

    builder
        .to_armored_string(&mut rng, ArmorOptions::default())
        .map_err(|_| crypto_err("armor the encrypted message"))
}

/// Parses a list of armored recipient public keys.
fn parse_recipients(armored: &[String]) -> Result<Vec<SignedPublicKey>, MailError> {
    armored
        .iter()
        .map(|armored| {
            SignedPublicKey::from_string(armored)
                .map(|(key, _)| key)
                .map_err(|_| invalid_key("recipient public key"))
        })
        .collect()
}

/// Decrypts an armored OpenPGP `message` with the secret key in
/// `secret_armored`, returning the recovered plaintext.
///
/// `passphrase` unlocks a passphrase-protected key; the passphrase is used only
/// for this call and never retained. A message not encrypted to this key, a
/// wrong passphrase, or corrupt ciphertext all fail as [`MailError::Crypto`].
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the secret key or the message armor
/// cannot be parsed, and [`MailError::Crypto`] if decryption fails.
pub fn decrypt_message(
    secret_armored: &str,
    passphrase: Option<&str>,
    message: &str,
) -> Result<DecryptedMessage, MailError> {
    let (secret, _headers) =
        SignedSecretKey::from_string(secret_armored).map_err(|_| invalid_key("secret key"))?;
    let (parsed, _headers) =
        Message::from_string(message).map_err(|_| invalid_key("encrypted message"))?;
    let password = passphrase.map_or_else(Password::empty, Password::from);

    let mut decrypted = parsed
        .decrypt(&password, &secret)
        .map_err(|_| crypto_err("decrypt the message"))?;
    let bytes = decrypted
        .as_data_vec()
        .map_err(|_| crypto_err("read the decrypted message"))?;
    Ok(DecryptedMessage { bytes })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pgp::generate_keypair;

    const PLAINTEXT: &[u8] = b"Content-Type: text/plain\r\n\r\nMeet at the bridge.\r\n";

    #[test]
    fn round_trips_to_a_single_recipient() {
        let recipient = generate_keypair("R <r@example.org>", None).unwrap();
        let armored = encrypt_message(
            std::slice::from_ref(&recipient.public_armored),
            &[],
            PLAINTEXT,
        )
        .unwrap();
        assert!(armored.contains("BEGIN PGP MESSAGE"));
        let out = decrypt_message(&recipient.secret_armored, None, &armored).unwrap();
        assert_eq!(out.bytes, PLAINTEXT);
    }

    #[test]
    fn any_of_several_recipients_can_decrypt() {
        let a = generate_keypair("A <a@example.org>", None).unwrap();
        let b = generate_keypair("B <b@example.org>", None).unwrap();
        let armored = encrypt_message(
            &[a.public_armored.clone(), b.public_armored.clone()],
            &[],
            PLAINTEXT,
        )
        .unwrap();
        assert_eq!(
            decrypt_message(&a.secret_armored, None, &armored)
                .unwrap()
                .bytes,
            PLAINTEXT
        );
        assert_eq!(
            decrypt_message(&b.secret_armored, None, &armored)
                .unwrap()
                .bytes,
            PLAINTEXT
        );
    }

    #[test]
    fn a_non_recipient_cannot_decrypt() {
        let recipient = generate_keypair("R <r@example.org>", None).unwrap();
        let stranger = generate_keypair("S <s@example.org>", None).unwrap();
        let armored = encrypt_message(&[recipient.public_armored], &[], PLAINTEXT).unwrap();
        let err = decrypt_message(&stranger.secret_armored, None, &armored).unwrap_err();
        assert!(matches!(err, MailError::Crypto { .. }));
    }

    #[test]
    fn passphrase_protected_recipient_round_trips() {
        let recipient = generate_keypair("R <r@example.org>", Some("open sesame")).unwrap();
        let armored = encrypt_message(
            std::slice::from_ref(&recipient.public_armored),
            &[],
            PLAINTEXT,
        )
        .unwrap();
        let out =
            decrypt_message(&recipient.secret_armored, Some("open sesame"), &armored).unwrap();
        assert_eq!(out.bytes, PLAINTEXT);
        // Wrong passphrase fails.
        let err = decrypt_message(&recipient.secret_armored, Some("wrong"), &armored).unwrap_err();
        assert!(matches!(err, MailError::Crypto { .. }));
    }

    #[test]
    fn hidden_recipient_key_id_is_not_leaked() {
        // SECURITY (finding #6): a Bcc'd (hidden) recipient's key id must not
        // appear in the encrypted message — its PKESK carries a wildcard key id,
        // so the ciphertext does not reveal that they were a recipient. A
        // To/Cc (visible) recipient's key id does appear.
        use std::io::{BufReader, Read};

        use pgp::armor::Dearmor;
        use pgp::composed::{Deserializable, SignedPublicKey};
        use pgp::packet::{Packet, PacketParser};
        use pgp::types::KeyDetails as _;

        let visible = generate_keypair("V <v@example.org>", None).unwrap();
        let hidden = generate_keypair("H <h@example.org>", None).unwrap();
        let armored = encrypt_message(
            std::slice::from_ref(&visible.public_armored),
            std::slice::from_ref(&hidden.public_armored),
            PLAINTEXT,
        )
        .unwrap();

        // Dearmor to raw packet bytes.
        let mut dearmor = Dearmor::new(BufReader::new(armored.as_bytes()));
        let mut bytes = Vec::new();
        dearmor.read_to_end(&mut bytes).unwrap();

        // The two recipients' encryption-subkey ids.
        let (vpub, _) = SignedPublicKey::from_string(&visible.public_armored).unwrap();
        let (hpub, _) = SignedPublicKey::from_string(&hidden.public_armored).unwrap();
        let v_id = vpub.public_subkeys[0].legacy_key_id();
        let h_id = hpub.public_subkeys[0].legacy_key_id();

        // Collect the key id carried by each PKESK packet.
        let mut pkesk_ids: Vec<Vec<u8>> = Vec::new();
        for packet in PacketParser::new(&bytes[..]) {
            if let Ok(Packet::PublicKeyEncryptedSessionKey(pkesk)) = packet {
                pkesk_ids.push(pkesk.id().unwrap().as_ref().to_vec());
            }
        }

        assert_eq!(pkesk_ids.len(), 2, "one PKESK per recipient");
        assert!(
            !pkesk_ids.iter().any(|id| id.as_slice() == h_id.as_ref()),
            "the Bcc (hidden) recipient's key id must not be revealed"
        );
        assert!(
            pkesk_ids.iter().any(|id| id.iter().all(|&b| b == 0)),
            "the hidden recipient's PKESK must use a wildcard key id"
        );
        assert!(
            pkesk_ids.iter().any(|id| id.as_slice() == v_id.as_ref()),
            "the visible recipient's key id should be present"
        );
    }

    #[test]
    fn hidden_recipient_can_still_decrypt() {
        let visible = generate_keypair("V <v@example.org>", None).unwrap();
        let hidden = generate_keypair("H <h@example.org>", None).unwrap();
        let armored = encrypt_message(
            &[visible.public_armored],
            &[hidden.public_armored],
            PLAINTEXT,
        )
        .unwrap();
        // The Bcc'd (hidden) recipient still decrypts via the anonymous PKESK.
        assert_eq!(
            decrypt_message(&hidden.secret_armored, None, &armored)
                .unwrap()
                .bytes,
            PLAINTEXT
        );
    }

    #[test]
    fn empty_recipient_list_is_rejected() {
        let err = encrypt_message(&[], &[], PLAINTEXT).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }
}
