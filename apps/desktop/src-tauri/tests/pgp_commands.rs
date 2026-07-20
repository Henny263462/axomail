//! Integration tests for the OpenPGP command layer: generate/list own keys with
//! keychain-backed secrets, import recipient keys, and the composer's
//! sign/encrypt availability report that drives the toggles.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::MailError;
use axomail_desktop_lib::commands::pgp::{
    compose_status_impl, generate_key_impl, import_recipient_impl, list_own_impl,
};
use axomail_security::SecretStore;
use axomail_security::pgp::{credential_ref_for_pgp, generate_keypair};
use axomail_storage::Store;
use common::secret_stores;

#[tokio::test]
async fn generate_stores_public_metadata_and_keeps_secret_in_keychain() {
    let store = Store::open_in_memory().await.unwrap();
    let (concrete, secrets) = secret_stores();

    let key = generate_key_impl(
        &store,
        &secrets,
        "Alice <alice@example.org>".to_owned(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(key.user_id, "Alice <alice@example.org>");
    assert!(key.is_default, "the first own key is the default");
    assert_eq!(key.fingerprint.len(), 40);

    // The public key is listed; the secret is only in the keychain.
    let listed = list_own_impl(&store).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].fingerprint == key.fingerprint);
    // The keychain holds the armored secret under the pgp/key/<fingerprint> ref;
    // the public list never carries secret material.
    let secret = concrete
        .get_secret(&credential_ref_for_pgp(&key.fingerprint))
        .unwrap()
        .expect("the secret key is stored in the keychain");
    assert!(secret.contains("PGP PRIVATE KEY"));
}

#[tokio::test]
async fn compose_status_reflects_key_availability() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();

    // No keys yet: cannot sign, cannot encrypt.
    let status = compose_status_impl(
        &store,
        "alice@example.org".to_owned(),
        vec!["bob@example.org".to_owned()],
    )
    .await
    .unwrap();
    assert!(!status.can_sign);
    assert!(!status.can_encrypt);
    assert_eq!(
        status.missing_recipients,
        vec!["bob@example.org".to_owned()]
    );

    // Give Alice an own key (enables signing) and import Bob's public key.
    generate_key_impl(
        &store,
        &secrets,
        "Alice <alice@example.org>".to_owned(),
        None,
    )
    .await
    .unwrap();
    let bob = generate_keypair("Bob <bob@example.org>", None).unwrap();
    import_recipient_impl(&store, "bob@example.org".to_owned(), bob.public_armored)
        .await
        .unwrap();

    let status = compose_status_impl(
        &store,
        "Alice <alice@example.org>".to_owned(),
        vec!["Bob <bob@example.org>".to_owned()],
    )
    .await
    .unwrap();
    assert!(status.can_sign, "an own key enables signing");
    assert!(status.can_encrypt, "every recipient has a key");
    assert!(status.missing_recipients.is_empty());

    // A second recipient without a key disables encryption and is reported.
    let status = compose_status_impl(
        &store,
        "alice@example.org".to_owned(),
        vec!["bob@example.org".to_owned(), "carol@example.org".to_owned()],
    )
    .await
    .unwrap();
    assert!(!status.can_encrypt);
    assert_eq!(
        status.missing_recipients,
        vec!["carol@example.org".to_owned()]
    );
}

#[tokio::test]
async fn importing_a_recipient_key_under_a_mismatched_address_is_rejected() {
    // SECURITY (finding #9): a key must actually certify the address it is
    // imported for. Importing Bob's key labelled as alice@ would let an
    // attacker's key be encrypted to when the user mails alice@ — so it is
    // rejected. A label the key *does* certify is accepted.
    let store = Store::open_in_memory().await.unwrap();
    let bob = generate_keypair("Bob <bob@example.org>", None).unwrap();

    let rejected = import_recipient_impl(
        &store,
        "alice@example.org".to_owned(),
        bob.public_armored.clone(),
    )
    .await;
    assert!(
        matches!(rejected, Err(MailError::Configuration { .. })),
        "a key not certifying alice@ must not bind to it, got {rejected:?}"
    );

    // The same key imported under an address it certifies succeeds.
    let accepted =
        import_recipient_impl(&store, "bob@example.org".to_owned(), bob.public_armored).await;
    assert!(
        accepted.is_ok(),
        "a matching label is accepted: {accepted:?}"
    );
}
