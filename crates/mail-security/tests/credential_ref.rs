//! Stability tests for the account credential-reference scheme.
#![allow(clippy::unwrap_used)]

use axomail_core::AccountId;
use axomail_security::{Direction, credential_ref_for_account};

#[test]
fn scheme_is_stable_and_direction_specific() {
    let id = AccountId::new(1);
    let incoming = credential_ref_for_account(id, Direction::Incoming);
    let outgoing = credential_ref_for_account(id, Direction::Outgoing);

    // Pin the exact strings: this is an on-disk contract and must not
    // change silently, or previously stored secrets become unreachable.
    assert_eq!(incoming.as_str(), "account/1/incoming");
    assert_eq!(outgoing.as_str(), "account/1/outgoing");
}

#[test]
fn different_accounts_get_different_references() {
    let a = credential_ref_for_account(AccountId::new(1), Direction::Incoming);
    let b = credential_ref_for_account(AccountId::new(2), Direction::Incoming);
    assert_ne!(a, b);
    assert_eq!(b.as_str(), "account/2/incoming");
}

#[test]
fn construction_is_deterministic() {
    let id = AccountId::new(42);
    let first = credential_ref_for_account(id, Direction::Outgoing);
    let second = credential_ref_for_account(id, Direction::Outgoing);
    assert_eq!(first, second);
    assert_eq!(first.as_str(), "account/42/outgoing");
}
