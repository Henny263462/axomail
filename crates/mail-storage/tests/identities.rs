//! Integration tests for the identity repository, focused on the
//! exactly-one-default invariant and the cascade from account deletion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::AccountId;
use axomail_storage::{
    ArchiveGranularity, ForwardMode, IdentityFields, IdentityId, NewIdentity, ReplyPosition,
    SignaturePlacement, Store,
};
use common::sample_account;

/// Builds identity fields with the given address; every preference at its
/// schema default so tests only vary what they assert on.
fn fields(email: &str) -> IdentityFields {
    IdentityFields {
        email: email.to_owned(),
        display_name: String::new(),
        reply_to: None,
        signature_text: None,
        signature_placement: SignaturePlacement::BelowQuote,
        reply_position: ReplyPosition::Below,
        forward_mode: ForwardMode::Inline,
        archive_granularity: ArchiveGranularity::Flat,
        sent_dest: None,
        drafts_dest: None,
        templates_dest: None,
        archive_dest: None,
    }
}

/// Builds a new identity for `account` at the given address.
fn new_identity(account: AccountId, email: &str, make_default: bool) -> NewIdentity {
    NewIdentity {
        account,
        fields: fields(email),
        make_default,
        sort_position: 0,
    }
}

/// Inserts a fresh account and returns its id.
async fn account(store: &Store, name: &str, email: &str) -> AccountId {
    store
        .accounts()
        .insert(&sample_account(name, email))
        .await
        .unwrap()
        .id
}

/// Counts an account's default identities; the invariant keeps this at one
/// whenever the account has any identity at all.
async fn default_count(store: &Store, account: AccountId) -> usize {
    store
        .identities()
        .list(account)
        .await
        .unwrap()
        .into_iter()
        .filter(|identity| identity.is_default)
        .count()
}

#[tokio::test]
async fn first_identity_is_default_even_when_not_requested() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;

    let first = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", false))
        .await
        .unwrap();
    assert!(first.is_default, "the first identity must be the default");

    let second = store
        .identities()
        .insert(&new_identity(acc, "sales@theferbers.eu", false))
        .await
        .unwrap();
    assert!(
        !second.is_default,
        "a later identity is not default by default"
    );
    assert_eq!(default_count(&store, acc).await, 1);
}

#[tokio::test]
async fn inserting_a_default_demotes_the_previous_one() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;

    let first = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();
    let second = store
        .identities()
        .insert(&new_identity(acc, "sales@theferbers.eu", true))
        .await
        .unwrap();

    let listed = store.identities().list(acc).await.unwrap();
    let first_now = listed.iter().find(|i| i.id == first.id).unwrap();
    let second_now = listed.iter().find(|i| i.id == second.id).unwrap();
    assert!(!first_now.is_default);
    assert!(second_now.is_default);
    assert_eq!(default_count(&store, acc).await, 1);
}

#[tokio::test]
async fn set_default_moves_the_flag_and_keeps_exactly_one() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;
    let first = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();
    let second = store
        .identities()
        .insert(&new_identity(acc, "sales@theferbers.eu", false))
        .await
        .unwrap();

    assert!(store.identities().set_default(second.id).await.unwrap());

    let listed = store.identities().list(acc).await.unwrap();
    assert!(!listed.iter().find(|i| i.id == first.id).unwrap().is_default);
    assert!(
        listed
            .iter()
            .find(|i| i.id == second.id)
            .unwrap()
            .is_default
    );
    assert_eq!(default_count(&store, acc).await, 1);
    // The default sorts first.
    assert_eq!(listed.first().unwrap().id, second.id);
}

#[tokio::test]
async fn set_default_on_unknown_identity_is_false() {
    let store = Store::open_in_memory().await.unwrap();
    assert!(
        !store
            .identities()
            .set_default(IdentityId::new(9999))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn deleting_the_default_promotes_a_survivor() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;
    let default = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();
    let other = store
        .identities()
        .insert(&new_identity(acc, "sales@theferbers.eu", false))
        .await
        .unwrap();

    assert!(store.identities().delete(default.id).await.unwrap());

    let listed = store.identities().list(acc).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, other.id);
    assert!(listed[0].is_default, "the survivor must become the default");
    assert_eq!(default_count(&store, acc).await, 1);
}

#[tokio::test]
async fn deleting_the_only_identity_leaves_the_account_empty() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;
    let only = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();

    assert!(store.identities().delete(only.id).await.unwrap());
    assert!(store.identities().list(acc).await.unwrap().is_empty());
    // Deleting an absent identity is a no-op.
    assert!(!store.identities().delete(only.id).await.unwrap());
}

#[tokio::test]
async fn deleting_the_account_cascades_to_its_identities() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;
    store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();
    store
        .identities()
        .insert(&new_identity(acc, "sales@theferbers.eu", false))
        .await
        .unwrap();

    assert!(store.accounts().delete(acc).await.unwrap());
    assert!(store.identities().list(acc).await.unwrap().is_empty());
}

#[tokio::test]
async fn update_rewrites_content_without_touching_the_default_flag() {
    let store = Store::open_in_memory().await.unwrap();
    let acc = account(&store, "Work", "klaus@theferbers.eu").await;
    let identity = store
        .identities()
        .insert(&new_identity(acc, "klaus@theferbers.eu", true))
        .await
        .unwrap();

    let mut updated = fields("klaus@theferbers.eu");
    updated.display_name = "Klaus Ferber".to_owned();
    updated.signature_text = Some("Sent from axomail".to_owned());
    updated.forward_mode = ForwardMode::Attachment;
    assert!(
        store
            .identities()
            .update(identity.id, &updated)
            .await
            .unwrap()
    );

    let got = store.identities().get(identity.id).await.unwrap().unwrap();
    assert_eq!(got.fields.display_name, "Klaus Ferber");
    assert_eq!(
        got.fields.signature_text.as_deref(),
        Some("Sent from axomail")
    );
    assert_eq!(got.fields.forward_mode, ForwardMode::Attachment);
    assert!(got.is_default, "update must leave the default flag alone");
}

#[tokio::test]
async fn list_all_spans_accounts_default_first_within_each() {
    let store = Store::open_in_memory().await.unwrap();
    let work = account(&store, "Work", "klaus@theferbers.eu").await;
    let home = account(&store, "Home", "klaus@home.example").await;
    store
        .identities()
        .insert(&new_identity(work, "klaus@theferbers.eu", true))
        .await
        .unwrap();
    store
        .identities()
        .insert(&new_identity(home, "klaus@home.example", true))
        .await
        .unwrap();
    let home_extra = store
        .identities()
        .insert(&new_identity(home, "family@home.example", false))
        .await
        .unwrap();

    let all = store.identities().list_all().await.unwrap();
    assert_eq!(all.len(), 3);
    // Grouped by account; within the Home account the default sorts ahead of
    // the extra identity.
    let home_ids: Vec<_> = all
        .iter()
        .filter(|i| i.account == home)
        .map(|i| i.id)
        .collect();
    assert_eq!(home_ids.len(), 2);
    assert_ne!(home_ids[0], home_extra.id, "default lists before the extra");
}
