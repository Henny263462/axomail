//! Integration tests for the PGP key repository: own-key insert with default
//! promotion, default switching, deletion with default re-promotion, and
//! recipient-key upsert / lookup used to decide whether encryption is possible.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_storage::{NewPgpOwnKey, NewPgpRecipientKey, Store};

fn own(fingerprint: &str, user_id: &str) -> NewPgpOwnKey {
    NewPgpOwnKey {
        fingerprint: fingerprint.to_owned(),
        key_id: fingerprint[fingerprint.len() - 16..].to_owned(),
        user_id: user_id.to_owned(),
        algorithm: "EdDSALegacy".to_owned(),
        public_armored: format!(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n{fingerprint}\n-----END PGP PUBLIC KEY BLOCK-----"
        ),
        has_passphrase: false,
        created_at_unix: 1_700_000_000,
        added_at_unix: 1_700_000_100,
    }
}

fn recipient(email: &str, fingerprint: &str) -> NewPgpRecipientKey {
    NewPgpRecipientKey {
        email: email.to_owned(),
        fingerprint: fingerprint.to_owned(),
        key_id: fingerprint[fingerprint.len() - 16..].to_owned(),
        user_id: format!("R <{email}>"),
        algorithm: "EdDSALegacy".to_owned(),
        public_armored: format!(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n{email}\n-----END PGP PUBLIC KEY BLOCK-----"
        ),
        created_at_unix: 1_700_000_000,
        added_at_unix: 1_700_000_100,
    }
}

#[tokio::test]
async fn first_own_key_becomes_default_and_switching_works() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.pgp_keys();

    let a = repo
        .insert_own(&own(
            "aaaa000000000000000000000000000000000001",
            "Alice <alice@example.org>",
        ))
        .await
        .unwrap();
    assert!(a.is_default, "the first key must be the default");

    let b = repo
        .insert_own(&own(
            "bbbb000000000000000000000000000000000002",
            "Alice Alt <alt@example.org>",
        ))
        .await
        .unwrap();
    assert!(!b.is_default, "a later key is not default");

    repo.set_default_own(b.id).await.unwrap();
    let default = repo.default_own().await.unwrap().unwrap();
    assert_eq!(default.id, b.id);
}

#[tokio::test]
async fn own_key_lookup_matches_by_email_then_default() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.pgp_keys();
    repo.insert_own(&own(
        "aaaa000000000000000000000000000000000001",
        "Alice <alice@example.org>",
    ))
    .await
    .unwrap();
    let alt = repo
        .insert_own(&own(
            "bbbb000000000000000000000000000000000002",
            "Bob <bob@example.org>",
        ))
        .await
        .unwrap();

    let by_email = repo
        .own_for_email("bob@example.org")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_email.id, alt.id);

    // An address with no matching key falls back to the default.
    let fallback = repo
        .own_for_email("nobody@example.org")
        .await
        .unwrap()
        .unwrap();
    assert!(fallback.is_default);
}

#[tokio::test]
async fn deleting_default_promotes_another_and_reports_the_removed_key() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.pgp_keys();
    let a = repo
        .insert_own(&own(
            "aaaa000000000000000000000000000000000001",
            "A <a@example.org>",
        ))
        .await
        .unwrap();
    repo.insert_own(&own(
        "bbbb000000000000000000000000000000000002",
        "B <b@example.org>",
    ))
    .await
    .unwrap();

    let removed = repo.delete_own(a.id).await.unwrap().unwrap();
    assert_eq!(removed.fingerprint, a.fingerprint);
    // A remaining key was promoted to default.
    assert!(repo.default_own().await.unwrap().unwrap().is_default);
    // Deleting a non-existent key returns None.
    assert!(repo.delete_own(a.id).await.unwrap().is_none());
}

#[tokio::test]
async fn recipient_upsert_is_idempotent_and_lookup_drives_encryption() {
    let store = Store::open_in_memory().await.unwrap();
    let repo = store.pgp_keys();

    assert!(!repo.has_recipient_key("bob@example.org").await.unwrap());

    repo.upsert_recipient(&recipient(
        "Bob@Example.org",
        "cccc000000000000000000000000000000000003",
    ))
    .await
    .unwrap();
    // Re-importing the same (email, fingerprint) updates rather than duplicates.
    repo.upsert_recipient(&recipient(
        "bob@example.org",
        "cccc000000000000000000000000000000000003",
    ))
    .await
    .unwrap();

    assert_eq!(repo.list_recipients().await.unwrap().len(), 1);
    // Email is normalised to lowercase, so a mixed-case lookup still hits.
    assert!(repo.has_recipient_key("bob@example.org").await.unwrap());
    assert!(
        repo.recipient_public_for("bob@example.org")
            .await
            .unwrap()
            .is_some()
    );
}
