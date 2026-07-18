//! Shared fixtures for the command integration tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axomail_core::{
    Address, ConnectionSecurity, EmailAddress, IncomingProtocol, MailboxId, MessageFlags,
    UtcTimestamp,
};
use axomail_desktop_lib::dto::{ComposeInput, CreateAccountInput, IncomingInput, ServerInput};
use axomail_desktop_lib::state::SharedSecretStore;
use axomail_security::InMemorySecretStore;
use axomail_storage::NewMessage;

/// Builds an in-memory secret store, returning both a concrete handle for
/// assertions and the shared trait-object handle the commands take.
pub fn secret_stores() -> (Arc<InMemorySecretStore>, SharedSecretStore) {
    let concrete = Arc::new(InMemorySecretStore::new());
    let shared: SharedSecretStore = concrete.clone();
    (concrete, shared)
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

/// Builds a compose payload for one account and recipient set.
pub fn compose_input(account_id: i64, to: &[&str], subject: &str) -> ComposeInput {
    ComposeInput {
        account_id,
        to: to.iter().map(|value| (*value).to_owned()).collect(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.to_owned(),
        text_body: "Hello from the axomail composer test.".to_owned(),
    }
}

/// A fully valid account-creation input.
pub fn valid_input() -> CreateAccountInput {
    CreateAccountInput {
        display_name: "Work".to_owned(),
        email: "klaus@theferbers.eu".to_owned(),
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
