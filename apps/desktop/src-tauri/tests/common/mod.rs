//! Shared fixtures for the command integration tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axomail_core::{
    Address, ConnectionSecurity, EmailAddress, IncomingProtocol, MailboxId, MessageFlags,
    UtcTimestamp,
};
use axomail_desktop_lib::dto::{CreateAccountInput, IncomingInput, ServerInput};
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
