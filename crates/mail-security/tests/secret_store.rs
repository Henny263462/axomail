//! Contract tests for the [`SecretStore`] trait via `InMemorySecretStore`.
#![allow(clippy::unwrap_used)]

use axomail_core::CredentialRef;
use axomail_security::{InMemorySecretStore, SecretStore};

fn reference() -> CredentialRef {
    CredentialRef::new("account/1/incoming")
}

#[test]
fn round_trips_a_stored_secret() {
    let store = InMemorySecretStore::new();
    store.set_secret(&reference(), "hunter2").unwrap();
    assert_eq!(
        store.get_secret(&reference()).unwrap().as_deref(),
        Some("hunter2")
    );
}

#[test]
fn overwrites_an_existing_secret() {
    let store = InMemorySecretStore::new();
    store.set_secret(&reference(), "first").unwrap();
    store.set_secret(&reference(), "second").unwrap();
    assert_eq!(
        store.get_secret(&reference()).unwrap().as_deref(),
        Some("second")
    );
}

#[test]
fn get_of_missing_key_returns_none() {
    let store = InMemorySecretStore::new();
    assert_eq!(store.get_secret(&reference()).unwrap(), None);
}

#[test]
fn delete_removes_the_secret() {
    let store = InMemorySecretStore::new();
    store.set_secret(&reference(), "secret").unwrap();
    store.delete_secret(&reference()).unwrap();
    assert_eq!(store.get_secret(&reference()).unwrap(), None);
}

#[test]
fn delete_of_missing_key_is_idempotent() {
    let store = InMemorySecretStore::new();
    // Deleting a reference that was never set must succeed.
    store.delete_secret(&reference()).unwrap();
    store.delete_secret(&reference()).unwrap();
    assert_eq!(store.get_secret(&reference()).unwrap(), None);
}

#[test]
fn distinct_references_do_not_collide() {
    let store = InMemorySecretStore::new();
    let incoming = CredentialRef::new("account/1/incoming");
    let outgoing = CredentialRef::new("account/1/outgoing");
    store.set_secret(&incoming, "in").unwrap();
    store.set_secret(&outgoing, "out").unwrap();
    assert_eq!(store.get_secret(&incoming).unwrap().as_deref(), Some("in"));
    assert_eq!(store.get_secret(&outgoing).unwrap().as_deref(), Some("out"));
}
