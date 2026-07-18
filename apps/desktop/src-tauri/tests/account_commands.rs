//! Integration tests for the account command surface.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, MailError, MailboxId, MailboxRole, MessageEnvelope, MessageFlags, MessageId,
};
use axomail_desktop_lib::commands::accounts::{
    create_account_impl, delete_account_impl, list_accounts_impl,
};
use axomail_desktop_lib::dto::{MessageListItemDto, checked_id};
use axomail_security::{Direction, SecretStore, credential_ref_for_account};
use axomail_storage::Store;
use common::{secret_stores, valid_input};

#[tokio::test]
async fn create_account_stores_secret_reference_and_seeds_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();

    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    // Five default mailboxes in the documented order, all with zero unread.
    let roles: Vec<MailboxRole> = summary
        .mailboxes
        .iter()
        .map(|mailbox| mailbox.role)
        .collect();
    assert_eq!(
        roles,
        vec![
            MailboxRole::Inbox,
            MailboxRole::Drafts,
            MailboxRole::Sent,
            MailboxRole::Archive,
            MailboxRole::Trash,
        ]
    );
    assert!(
        summary
            .mailboxes
            .iter()
            .all(|mailbox| mailbox.unread_count == 0)
    );

    let account_id = AccountId::new(summary.id);
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);

    // The passwords landed in the keychain under their id-based references.
    assert_eq!(
        secrets_concrete
            .get_secret(&incoming_ref)
            .unwrap()
            .as_deref(),
        Some("incoming-secret")
    );
    assert_eq!(
        secrets_concrete
            .get_secret(&outgoing_ref)
            .unwrap()
            .as_deref(),
        Some("outgoing-secret")
    );

    // The row references the secret; it never stores the password itself.
    let account = store
        .accounts()
        .get(account_id)
        .await
        .unwrap()
        .expect("account row exists");
    assert_eq!(account.incoming.credential, incoming_ref);
    assert_eq!(account.outgoing.credential, outgoing_ref);
    assert_ne!(account.incoming.credential.as_str(), "incoming-secret");

    // The mailboxes are real rows, not fixtures.
    assert_eq!(store.mailboxes().list(account_id).await.unwrap().len(), 5);
}

#[tokio::test]
async fn create_account_rejects_invalid_email_without_storing_a_secret() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();
    let mut input = valid_input();
    input.email = "not-an-address".to_owned();

    let error = create_account_impl(&store, &secrets, input)
        .await
        .unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));

    assert!(store.accounts().list().await.unwrap().is_empty());
    assert_no_secret_stored(&secrets_concrete);
}

#[tokio::test]
async fn create_account_rejects_invalid_host_without_storing_a_secret() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();
    let mut input = valid_input();
    input.incoming.host = String::new();

    let error = create_account_impl(&store, &secrets, input)
        .await
        .unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));

    assert!(store.accounts().list().await.unwrap().is_empty());
    assert_no_secret_stored(&secrets_concrete);
}

#[tokio::test]
async fn delete_account_removes_rows_and_secrets() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account_id = AccountId::new(summary.id);
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);

    delete_account_impl(&store, &secrets, summary.id)
        .await
        .unwrap();

    assert!(store.accounts().get(account_id).await.unwrap().is_none());
    assert!(store.mailboxes().list(account_id).await.unwrap().is_empty());
    assert!(
        secrets_concrete
            .get_secret(&incoming_ref)
            .unwrap()
            .is_none()
    );
    assert!(
        secrets_concrete
            .get_secret(&outgoing_ref)
            .unwrap()
            .is_none()
    );

    // Deleting an already-absent account is a no-op, not an error.
    delete_account_impl(&store, &secrets, summary.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn list_accounts_reports_mailboxes() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let accounts = list_accounts_impl(&store).await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].email, "klaus@theferbers.eu");
    assert_eq!(accounts[0].mailboxes.len(), 5);
    assert!(
        accounts[0]
            .mailboxes
            .iter()
            .all(|mailbox| mailbox.unread_count == 0)
    );
}

#[test]
fn checked_id_rejects_values_outside_the_safe_range() {
    assert!(checked_id(0).is_ok());
    assert!(checked_id(9_007_199_254_740_991).is_ok());
    assert!(checked_id(9_007_199_254_740_992).is_err());
    assert!(checked_id(-1).is_err());
}

#[test]
fn dto_builder_rejects_an_oversized_id() {
    let envelope = MessageEnvelope {
        id: MessageId::new(9_007_199_254_740_992),
        mailbox: MailboxId::new(1),
        remote_id: None,
        subject: None,
        from: vec![],
        to: vec![],
        date: None,
        flags: MessageFlags::default(),
        size_bytes: None,
        preview: None,
    };
    let error = MessageListItemDto::from_envelope(&envelope).unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));
}

/// Asserts neither credential reference for the first account holds a secret.
fn assert_no_secret_stored(secrets: &axomail_security::InMemorySecretStore) {
    let incoming = credential_ref_for_account(AccountId::new(1), Direction::Incoming);
    let outgoing = credential_ref_for_account(AccountId::new(1), Direction::Outgoing);
    assert!(secrets.get_secret(&incoming).unwrap().is_none());
    assert!(secrets.get_secret(&outgoing).unwrap().is_none());
}
