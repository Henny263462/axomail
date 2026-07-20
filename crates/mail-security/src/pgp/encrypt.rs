//! OpenPGP message encryption and decryption over raw byte payloads.
//!
//! These operate on opaque bytes: the caller decides what the plaintext is. For
//! PGP/MIME (RFC 3156) the encrypted payload is the inner MIME entity — which,
//! for a signed-and-encrypted message, is itself a `multipart/signed` block, so
//! signature verification is a separate step performed on the decrypted bytes.

use pgp::composed::{
    ArmorOptions, Deserializable, Message, MessageBuilder, SignedPublicKey, SignedSecretKey,
};
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::types::{KeyDetails, Password};

use super::{crypto_err, invalid_key};
use axomail_core::MailError;

/// The plaintext recovered from an encrypted OpenPGP message.
#[derive(Debug, Clone)]
pub struct DecryptedMessage {
    /// Decrypted payload bytes (for PGP/MIME, the inner MIME entity).
    pub bytes: Vec<u8>,
}

/// Encrypts `content` to every recipient in `recipient_public_armored`,
/// returning an ASCII-armored OpenPGP message.
///
/// Encryption is public-key (SEIPD v1 / AES-256): each recipient's public key
/// wraps the session key, so any one recipient's secret key decrypts it. At
/// least one recipient key is required — encrypting to nobody is rejected so a
/// message can never be silently unreadable.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the recipient list is empty or a key
/// cannot be parsed, and [`MailError::Crypto`] if encryption or armoring fails.
pub fn encrypt_message(
    recipient_public_armored: &[String],
    content: &[u8],
) -> Result<String, MailError> {
    if recipient_public_armored.is_empty() {
        return Err(MailError::Configuration {
            message: "encryption needs at least one recipient public key".to_owned(),
        });
    }
    let recipients: Vec<SignedPublicKey> = recipient_public_armored
        .iter()
        .map(|armored| {
            SignedPublicKey::from_string(armored)
                .map(|(key, _)| key)
                .map_err(|_| invalid_key("recipient public key"))
        })
        .collect::<Result<_, _>>()?;

    let mut rng = rand::thread_rng();
    let mut builder = MessageBuilder::from_bytes("", content.to_vec())
        .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
    for recipient in &recipients {
        // Encrypt to the recipient's encryption-capable subkey (e.g. the ECDH
        // subkey of an Ed25519 cert); fall back to the primary key only when it
        // is itself an encryption key (e.g. a single RSA key).
        if let Some(subkey) = recipient
            .public_subkeys
            .iter()
            .find(|subkey| subkey.algorithm().can_encrypt())
        {
            builder
                .encrypt_to_key(&mut rng, subkey)
                .map_err(|_| crypto_err("encrypt to a recipient key"))?;
        } else if recipient.algorithm().can_encrypt() {
            builder
                .encrypt_to_key(&mut rng, recipient)
                .map_err(|_| crypto_err("encrypt to a recipient key"))?;
        } else {
            return Err(MailError::Configuration {
                message: "a recipient key has no encryption-capable subkey".to_owned(),
            });
        }
    }
    builder
        .to_armored_string(&mut rng, ArmorOptions::default())
        .map_err(|_| crypto_err("armor the encrypted message"))
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
        let armored =
            encrypt_message(std::slice::from_ref(&recipient.public_armored), PLAINTEXT).unwrap();
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
        let armored = encrypt_message(&[recipient.public_armored], PLAINTEXT).unwrap();
        let err = decrypt_message(&stranger.secret_armored, None, &armored).unwrap_err();
        assert!(matches!(err, MailError::Crypto { .. }));
    }

    #[test]
    fn passphrase_protected_recipient_round_trips() {
        let recipient = generate_keypair("R <r@example.org>", Some("open sesame")).unwrap();
        let armored =
            encrypt_message(std::slice::from_ref(&recipient.public_armored), PLAINTEXT).unwrap();
        let out =
            decrypt_message(&recipient.secret_armored, Some("open sesame"), &armored).unwrap();
        assert_eq!(out.bytes, PLAINTEXT);
        // Wrong passphrase fails.
        let err = decrypt_message(&recipient.secret_armored, Some("wrong"), &armored).unwrap_err();
        assert!(matches!(err, MailError::Crypto { .. }));
    }

    #[test]
    fn empty_recipient_list_is_rejected() {
        let err = encrypt_message(&[], PLAINTEXT).unwrap_err();
        assert!(matches!(err, MailError::Configuration { .. }));
    }
}
