//! Shared fixtures for the command integration tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod pop3_server;
pub mod provider;

use std::sync::Arc;

use axomail_core::{
    AccountId, Address, ConnectionSecurity, EmailAddress, IncomingProtocol, MailboxId, MailboxName,
    MailboxRole, MessageFlags, UtcTimestamp,
};
use axomail_desktop_lib::dto::{
    AuthInput, ComposeInput, CreateAccountInput, IncomingInput, ServerInput,
};
use axomail_desktop_lib::state::SharedSecretStore;
use axomail_security::InMemorySecretStore;
use axomail_storage::{NewMailbox, NewMessage, Store};

/// Builds an in-memory secret store, returning both a concrete handle for
/// assertions and the shared trait-object handle the commands take.
pub fn secret_stores() -> (Arc<InMemorySecretStore>, SharedSecretStore) {
    let concrete = Arc::new(InMemorySecretStore::new());
    let shared: SharedSecretStore = concrete.clone();
    (concrete, shared)
}

/// Seeds the four non-inbox standard folders on an account.
///
/// IMAP accounts now seed only the inbox (folder discovery fills the rest), so
/// tests that need Drafts/Sent/Archive/Trash present up front add them here.
/// Inserted in the classic display order right after the seeded inbox, so a
/// re-listed account reads `[Inbox, Drafts, Sent, Archive, Trash]`.
pub async fn seed_standard_folders(store: &Store, account: AccountId) {
    for (name, role) in [
        ("Drafts", MailboxRole::Drafts),
        ("Sent", MailboxRole::Sent),
        ("Archive", MailboxRole::Archive),
        ("Trash", MailboxRole::Trash),
    ] {
        store
            .mailboxes()
            .upsert(&NewMailbox {
                account,
                name: MailboxName::new(name),
                remote_name: MailboxName::new(name),
                role,
                uid_validity: None,
                uid_next: None,
            })
            .await
            .unwrap();
    }
}

/// A valid account-creation input whose SMTP server points at a local test
/// server on `127.0.0.1:outgoing_port` over implicit TLS.
pub fn outgoing_account_input(outgoing_port: u16) -> CreateAccountInput {
    let mut input = valid_input();
    input.outgoing = ServerInput {
        host: "127.0.0.1".to_owned(),
        port: outgoing_port,
        security: ConnectionSecurity::Tls,
        username: "user".to_owned(),
        password: "pass".to_owned(),
    };
    input
}

/// A valid account-creation input whose incoming server is POP3 over implicit
/// TLS at `127.0.0.1:incoming_port`, keeping the default outgoing SMTP config.
pub fn pop3_account_input(incoming_port: u16) -> CreateAccountInput {
    let mut input = valid_input();
    input.incoming = IncomingInput {
        protocol: IncomingProtocol::Pop3,
        host: "127.0.0.1".to_owned(),
        port: incoming_port,
        security: ConnectionSecurity::Tls,
        username: "user".to_owned(),
        password: "pass".to_owned(),
    };
    input
}

/// Builds a compose payload for one account and recipient set.
pub fn compose_input(account_id: i64, to: &[&str], subject: &str) -> ComposeInput {
    ComposeInput {
        account_id,
        identity_id: None,
        to: to.iter().map(|value| (*value).to_owned()).collect(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.to_owned(),
        text_body: "Hello from the axomail composer test.".to_owned(),
        html_body: None,
        attachments: Vec::new(),
        scheduled_at_unix: None,
    }
}

/// A fully valid account-creation input.
pub fn valid_input() -> CreateAccountInput {
    CreateAccountInput {
        display_name: "Work".to_owned(),
        email: "klaus@theferbers.eu".to_owned(),
        auth: AuthInput::default(),
        incoming: IncomingInput {
            protocol: IncomingProtocol::Imap,
            host: "imap.example.org".to_owned(),
            port: 993,
            security: ConnectionSecurity::Tls,
            username: "klaus@theferbers.eu".to_owned(),
            password: "incoming-secret".to_owned(),
        },
        outgoing: ServerInput {
            host: "smtp.example.org".to_owned(),
            port: 465,
            security: ConnectionSecurity::Tls,
            username: "klaus@theferbers.eu".to_owned(),
            password: "outgoing-secret".to_owned(),
        },
    }
}

/// A valid account-creation input that authenticates with OAuth2 (Gmail
/// preset). No password is present; tokens are obtained later by the browser
/// sign-in flow.
pub fn oauth_account_input() -> CreateAccountInput {
    use axomail_desktop_lib::dto::OAuthInput;
    let mut input = valid_input();
    input.auth = AuthInput {
        mechanism: axomail_core::AuthMechanism::OAuth2,
        oauth: Some(OAuthInput {
            provider: axomail_core::OAuthProvider::Gmail,
            client_id: "123.apps.googleusercontent.com".to_owned(),
            auth_url: None,
            token_url: None,
            scopes: None,
        }),
    };
    // Passwords are irrelevant for OAuth accounts and must not be stored.
    input.incoming.password = String::new();
    input.outgoing.password = String::new();
    input
}

/// A message to insert directly through the storage layer for list tests.
pub fn sample_message(mailbox: MailboxId, subject: &str, date: Option<i64>) -> NewMessage {
    NewMessage {
        mailbox,
        remote_id: None,
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: Some("Ada Lovelace".to_owned()),
            email: EmailAddress::parse("ada@example.org").expect("valid address"),
        }],
        to: vec![Address {
            display_name: None,
            email: EmailAddress::parse("klaus@theferbers.eu").expect("valid address"),
        }],
        cc: vec![],
        bcc: vec![],
        date: date.map(UtcTimestamp::from_unix_seconds),
        flags: MessageFlags::default(),
        size_bytes: Some(1024),
        preview: Some("preview text".to_owned()),
        has_raw: false,
    }
}
