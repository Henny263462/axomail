//! Integration tests for the account command surface.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountAuth, AccountId, AuthMechanism, ConnectionSecurity, IncomingProtocol, MailError,
    MailboxId, MailboxRole, MessageEnvelope, MessageFlags, MessageId, ThreadingHeaders,
};
use axomail_desktop_lib::commands::accounts::{
    create_account_impl, delete_account_impl, list_account_details_impl, list_accounts_impl,
    update_account_impl,
};
use axomail_desktop_lib::dto::{
    MessageListItemDto, ServerEndpointInput, UpdateAccountInput, checked_id,
};
use axomail_security::{Direction, SecretStore, credential_ref_for_account};
use axomail_storage::Store;
use common::{oauth_account_input, sample_message, secret_stores, valid_input};

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
async fn create_oauth_account_stores_config_but_no_secret() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();

    let summary = create_account_impl(&store, &secrets, oauth_account_input())
        .await
        .unwrap();
    assert_eq!(summary.auth_mechanism, AuthMechanism::OAuth2);

    let account_id = AccountId::new(summary.id);
    let account = store
        .accounts()
        .get(account_id)
        .await
        .unwrap()
        .expect("account row exists");

    // The Gmail preset was resolved into concrete endpoints.
    let AccountAuth::OAuth2(config) = &account.auth else {
        panic!("expected an OAuth2 account");
    };
    assert_eq!(config.client_id.as_str(), "123.apps.googleusercontent.com");
    assert_eq!(
        config.token_url.as_str(),
        "https://oauth2.googleapis.com/token"
    );

    // Crucially, no secret is stored at creation: the tokens come from the
    // separate browser sign-in.
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);
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

    // The account nonetheless shows up in the list with its OAuth mechanism.
    let listed = list_accounts_impl(&store).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].auth_mechanism, AuthMechanism::OAuth2);
}

#[tokio::test]
async fn create_oauth_account_without_config_is_rejected() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let mut input = oauth_account_input();
    // Select OAuth2 but drop the configuration.
    input.auth = axomail_desktop_lib::dto::AuthInput {
        mechanism: AuthMechanism::OAuth2,
        oauth: None,
    };

    let error = create_account_impl(&store, &secrets, input)
        .await
        .unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));
    assert!(store.accounts().list().await.unwrap().is_empty());
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
    let raw_root = tempfile::tempdir().unwrap();

    delete_account_impl(&store, &secrets, raw_root.path(), summary.id)
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
    delete_account_impl(&store, &secrets, raw_root.path(), summary.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_account_cascades_to_messages() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account_id = AccountId::new(summary.id);
    let inbox = MailboxId::new(summary.mailboxes[0].id);
    let message = store
        .messages()
        .insert(&sample_message(inbox, "keep-me", Some(1)))
        .await
        .unwrap();
    let raw_root = tempfile::tempdir().unwrap();

    delete_account_impl(&store, &secrets, raw_root.path(), summary.id)
        .await
        .unwrap();

    // The row deletion cascades through mailboxes to the messages stored in
    // them: the account, its mailboxes, and the message are all gone.
    assert!(store.accounts().get(account_id).await.unwrap().is_none());
    assert!(store.mailboxes().list(account_id).await.unwrap().is_empty());
    assert!(store.messages().get(message.id).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_account_removes_orphaned_raw_message_bodies() {
    use axomail_desktop_lib::commands::detail::raw_message_path;

    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let raw_root = tempfile::tempdir().unwrap();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = MailboxId::new(summary.mailboxes[0].id);

    // One message whose raw body was cached to disk, and a second whose body
    // was never fetched (its raw file is absent).
    let cached = store
        .messages()
        .insert(&sample_message(inbox, "cached", Some(1)))
        .await
        .unwrap();
    let uncached = store
        .messages()
        .insert(&sample_message(inbox, "uncached", Some(2)))
        .await
        .unwrap();

    let cached_path = raw_message_path(raw_root.path(), cached.id.value());
    let uncached_path = raw_message_path(raw_root.path(), uncached.id.value());
    std::fs::create_dir_all(cached_path.parent().unwrap()).unwrap();
    std::fs::write(&cached_path, b"raw bytes").unwrap();
    assert!(cached_path.exists());

    // Deletion succeeds despite the second message's missing raw file, and the
    // cached file is gone afterwards.
    delete_account_impl(&store, &secrets, raw_root.path(), summary.id)
        .await
        .unwrap();

    assert!(
        !cached_path.exists(),
        "the orphaned raw body must be deleted"
    );
    assert!(!uncached_path.exists());
}

#[tokio::test]
async fn update_account_changes_servers_and_preserves_protocol_and_credentials() {
    let store = Store::open_in_memory().await.unwrap();
    let (secrets_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let input = UpdateAccountInput {
        account_id: summary.id,
        display_name: "Renamed".to_owned(),
        incoming: ServerEndpointInput {
            host: "imap.new.example.org".to_owned(),
            port: 143,
            security: ConnectionSecurity::StartTls,
        },
        outgoing: ServerEndpointInput {
            host: "smtp.new.example.org".to_owned(),
            port: 2525,
            security: ConnectionSecurity::StartTls,
        },
    };
    let detail = update_account_impl(&store, input).await.unwrap();

    // The edited fields are reflected in the returned detail.
    assert_eq!(detail.display_name, "Renamed");
    assert_eq!(detail.incoming.host, "imap.new.example.org");
    assert_eq!(detail.incoming.port, 143);
    assert_eq!(detail.incoming.security, ConnectionSecurity::StartTls);
    assert_eq!(detail.outgoing.host, "smtp.new.example.org");
    assert_eq!(detail.outgoing.port, 2525);
    assert_eq!(detail.outgoing.security, ConnectionSecurity::StartTls);

    // Protocol, login names, and email are immutable through this path.
    assert_eq!(detail.email, "klaus@theferbers.eu");
    assert_eq!(detail.incoming.protocol, IncomingProtocol::Imap);
    assert_eq!(detail.incoming.username, "klaus@theferbers.eu");
    assert_eq!(detail.outgoing.username, "klaus@theferbers.eu");

    // The stored secrets are untouched: an edit performs no keychain I/O.
    let account_id = AccountId::new(summary.id);
    let incoming_ref = credential_ref_for_account(account_id, Direction::Incoming);
    let outgoing_ref = credential_ref_for_account(account_id, Direction::Outgoing);
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
}

#[tokio::test]
async fn update_account_rejects_an_invalid_host_and_changes_nothing() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let input = UpdateAccountInput {
        account_id: summary.id,
        display_name: "Renamed".to_owned(),
        incoming: ServerEndpointInput {
            host: String::new(),
            port: 143,
            security: ConnectionSecurity::Tls,
        },
        outgoing: ServerEndpointInput {
            host: "smtp.example.org".to_owned(),
            port: 465,
            security: ConnectionSecurity::Tls,
        },
    };
    let error = update_account_impl(&store, input).await.unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));

    // The rejected edit left the stored configuration untouched.
    let details = list_account_details_impl(&store).await.unwrap();
    assert_eq!(details[0].display_name, "Work");
    assert_eq!(details[0].incoming.host, "imap.example.org");
}

#[tokio::test]
async fn update_account_rejects_an_unknown_id() {
    let store = Store::open_in_memory().await.unwrap();

    let input = UpdateAccountInput {
        account_id: 4242,
        display_name: "Ghost".to_owned(),
        incoming: ServerEndpointInput {
            host: "imap.example.org".to_owned(),
            port: 993,
            security: ConnectionSecurity::Tls,
        },
        outgoing: ServerEndpointInput {
            host: "smtp.example.org".to_owned(),
            port: 465,
            security: ConnectionSecurity::Tls,
        },
    };
    let error = update_account_impl(&store, input).await.unwrap_err();
    assert!(matches!(error, MailError::Configuration { .. }));
}

#[tokio::test]
async fn list_account_details_reports_editable_configuration() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let details = list_account_details_impl(&store).await.unwrap();
    assert_eq!(details.len(), 1);
    let detail = &details[0];
    assert_eq!(detail.email, "klaus@theferbers.eu");
    assert_eq!(detail.incoming.protocol, IncomingProtocol::Imap);
    assert_eq!(detail.incoming.host, "imap.example.org");
    assert_eq!(detail.incoming.port, 993);
    assert_eq!(detail.incoming.security, ConnectionSecurity::Tls);
    assert_eq!(detail.outgoing.host, "smtp.example.org");
    assert_eq!(detail.outgoing.port, 465);
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
        threading: ThreadingHeaders::default(),
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
