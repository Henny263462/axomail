//! End-to-end RFC 3156 PGP/MIME round-trips: wrap an outgoing message as
//! `multipart/signed` and/or `multipart/encrypted`, then recognise, verify, and
//! decrypt it back — proving the on-the-wire structure and byte-exact signature
//! coverage interoperate with the crypto primitives.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_security::pgp::{
    PgpMimeMessage, SignatureVerification, apply_pgp_mime, decrypt_message, generate_keypair,
    parse_pgp_mime, verify_detached,
};

/// A minimal RFC 5322 message with envelope headers and a text/plain body.
const SOURCE: &[u8] = b"From: Alice <alice@example.org>\r\n\
To: Bob <bob@example.org>\r\n\
Subject: Treaty\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
Content-Transfer-Encoding: 7bit\r\n\
\r\n\
The treaty is ready for your signature.\r\n";

fn to_str(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn sign_only_produces_verifiable_multipart_signed() {
    let key = generate_keypair("Alice <alice@example.org>", None).unwrap();
    let wrapped = apply_pgp_mime(SOURCE, Some((&key.secret_armored, None)), &[]).unwrap();
    let text = to_str(&wrapped);

    // Top-level envelope headers survive; the content becomes multipart/signed.
    assert!(text.contains("Subject: Treaty"));
    assert!(text.contains("multipart/signed"));
    assert!(text.contains("protocol=\"application/pgp-signature\""));
    assert!(text.contains("micalg=\"pgp-sha256\""));

    let parsed = parse_pgp_mime(&wrapped).expect("recognised as PGP/MIME");
    let PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    } = parsed
    else {
        panic!("expected SignedMime, got {parsed:?}");
    };

    // The signed entity is exactly the inner MIME entity (content headers +
    // body), and it verifies against the signer's public key.
    let signed_text = to_str(&signed_entity);
    assert!(signed_text.contains("Content-Type: text/plain"));
    assert!(signed_text.contains("The treaty is ready"));
    assert!(
        !signed_text.contains("Subject:"),
        "envelope leaked into signed body"
    );

    let status = verify_detached(&[key.public_armored], &signature, &signed_entity).unwrap();
    assert_eq!(status, SignatureVerification::Valid);
}

#[test]
fn tampering_with_the_signed_body_is_detected() {
    let key = generate_keypair("Alice <alice@example.org>", None).unwrap();
    let wrapped = apply_pgp_mime(SOURCE, Some((&key.secret_armored, None)), &[]).unwrap();
    let PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    } = parse_pgp_mime(&wrapped).unwrap()
    else {
        panic!("expected SignedMime");
    };

    // Flip one byte of the covered content.
    let mut tampered = signed_entity.clone();
    let pos = tampered.len() - 5;
    tampered[pos] ^= 0x20;
    let status = verify_detached(&[key.public_armored], &signature, &tampered).unwrap();
    assert_eq!(status, SignatureVerification::Invalid);
}

#[test]
fn encrypt_only_round_trips_the_inner_entity() {
    let recipient = generate_keypair("Bob <bob@example.org>", None).unwrap();
    let wrapped = apply_pgp_mime(
        SOURCE,
        None,
        std::slice::from_ref(&recipient.public_armored),
    )
    .unwrap();
    let text = to_str(&wrapped);
    assert!(text.contains("Subject: Treaty"));
    assert!(text.contains("multipart/encrypted"));
    assert!(text.contains("protocol=\"application/pgp-encrypted\""));

    let PgpMimeMessage::EncryptedMime { ciphertext } = parse_pgp_mime(&wrapped).unwrap() else {
        panic!("expected EncryptedMime");
    };
    let decrypted = decrypt_message(&recipient.secret_armored, None, &ciphertext).unwrap();
    let inner = to_str(&decrypted.bytes);
    assert!(inner.contains("Content-Type: text/plain"));
    assert!(inner.contains("The treaty is ready"));
}

#[test]
fn sign_then_encrypt_round_trips_and_verifies() {
    let alice = generate_keypair("Alice <alice@example.org>", None).unwrap();
    let bob = generate_keypair("Bob <bob@example.org>", None).unwrap();

    // Alice signs and encrypts to Bob (and, as a client would, to herself).
    let wrapped = apply_pgp_mime(
        SOURCE,
        Some((&alice.secret_armored, None)),
        &[bob.public_armored.clone(), alice.public_armored.clone()],
    )
    .unwrap();
    assert!(to_str(&wrapped).contains("multipart/encrypted"));

    // Bob decrypts the outer layer, revealing a multipart/signed entity.
    let PgpMimeMessage::EncryptedMime { ciphertext } = parse_pgp_mime(&wrapped).unwrap() else {
        panic!("expected EncryptedMime");
    };
    let decrypted = decrypt_message(&bob.secret_armored, None, &ciphertext).unwrap();
    let inner = parse_pgp_mime(&decrypted.bytes).expect("inner is PGP/MIME signed");
    let PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    } = inner
    else {
        panic!("expected inner SignedMime");
    };

    // The signature verifies against Alice's key, and the plaintext is intact.
    let status = verify_detached(&[alice.public_armored], &signature, &signed_entity).unwrap();
    assert_eq!(status, SignatureVerification::Valid);
    assert!(to_str(&signed_entity).contains("The treaty is ready"));
}

#[test]
fn passphrase_protected_signer_round_trips() {
    let key = generate_keypair("Alice <alice@example.org>", Some("pass")).unwrap();
    let wrapped = apply_pgp_mime(SOURCE, Some((&key.secret_armored, Some("pass"))), &[]).unwrap();
    let PgpMimeMessage::SignedMime {
        signed_entity,
        signature,
    } = parse_pgp_mime(&wrapped).unwrap()
    else {
        panic!("expected SignedMime");
    };
    let status = verify_detached(&[key.public_armored], &signature, &signed_entity).unwrap();
    assert_eq!(status, SignatureVerification::Valid);
}

#[test]
fn no_protection_returns_the_message_unchanged() {
    let unchanged = apply_pgp_mime(SOURCE, None, &[]).unwrap();
    assert_eq!(unchanged, SOURCE);
}
