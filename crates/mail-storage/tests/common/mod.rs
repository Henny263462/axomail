//! Shared fixtures for the storage integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU16;

use axomail_core::{
    Address, ConnectionSecurity, CredentialRef, EmailAddress, IncomingProtocol,
    IncomingServerConfig, MailboxName, MailboxRole, MessageFlags, OutgoingServerConfig,
    ServerEndpoint, UtcTimestamp,
};
use axomail_storage::{NewAccount, NewMailbox, NewMessage};

/// Builds a valid account fixture with distinct incoming/outgoing config
/// and non-secret credential references.
pub fn sample_account(display_name: &str, email: &str) -> NewAccount {
    let port = |value: u16| NonZeroU16::new(value).unwrap();
    NewAccount {
        display_name: display_name.to_owned(),
        email: EmailAddress::parse(email).unwrap(),
        incoming: IncomingServerConfig {
            protocol: IncomingProtocol::Imap,
            endpoint: ServerEndpoint::new("imap.example.org", port(993)).unwrap(),
            security: ConnectionSecurity::Tls,
            username: email.to_owned(),
            credential: CredentialRef::new("axomail/account/incoming"),
        },
        outgoing: OutgoingServerConfig {
            endpoint: ServerEndpoint::new("smtp.example.org", port(465)).unwrap(),
            security: ConnectionSecurity::StartTls,
            username: email.to_owned(),
            credential: CredentialRef::new("axomail/account/outgoing"),
        },
    }
}

/// Builds a mailbox fixture for `account`.
pub fn sample_mailbox(account: axomail_core::AccountId, name: &str) -> NewMailbox {
    NewMailbox {
        account,
        name: MailboxName::new(name),
        role: MailboxRole::guess_from_name(name),
        uid_validity: Some(1),
        uid_next: Some(1),
    }
}

/// Builds a message fixture in `mailbox` with the given subject and date.
pub fn sample_message(
    mailbox: axomail_core::MailboxId,
    subject: &str,
    date_unix: Option<i64>,
) -> NewMessage {
    let from = Address {
        display_name: Some("Ada Lovelace".to_owned()),
        email: EmailAddress::parse("ada@example.org").unwrap(),
    };
    let to = Address::from_email(EmailAddress::parse("klaus@theferbers.eu").unwrap());
    NewMessage {
        mailbox,
        remote_id: None,
        subject: Some(subject.to_owned()),
        from: vec![from],
        to: vec![to],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: date_unix.map(UtcTimestamp::from_unix_seconds),
        flags: MessageFlags::default(),
        size_bytes: Some(2048),
        preview: Some("preview text".to_owned()),
        has_raw: false,
    }
}
