//! Shared fixtures for the storage integration tests.
// Each integration-test binary pulls in this whole module; fixtures used
// by only one binary are otherwise flagged dead in the others.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::num::NonZeroU16;

use axomail_core::{
    AccountAuth, Address, ConnectionSecurity, CredentialRef, EmailAddress, IncomingProtocol,
    IncomingServerConfig, MailboxName, MailboxRole, MessageFlags, OutgoingServerConfig,
    RemoteEnvelope, RemoteMessageId, ServerEndpoint, ThreadingHeaders, UtcTimestamp,
};
use axomail_storage::{NewAccount, NewMailbox, NewMessage};

/// Builds a valid account fixture with distinct incoming/outgoing config
/// and non-secret credential references.
pub fn sample_account(display_name: &str, email: &str) -> NewAccount {
    let port = |value: u16| NonZeroU16::new(value).unwrap();
    NewAccount {
        display_name: display_name.to_owned(),
        email: EmailAddress::parse(email).unwrap(),
        auth: AccountAuth::Password,
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

/// Builds a valid OAuth2 account fixture. The endpoints and scopes mirror a
/// Gmail-style configuration; no secret material is present (tokens live in
/// the keychain, never in the row).
pub fn sample_oauth_account(display_name: &str, email: &str) -> NewAccount {
    use axomail_core::{HttpsUrl, OAuthClientId, OAuthConfig, OAuthScopes};
    let port = |value: u16| NonZeroU16::new(value).unwrap();
    NewAccount {
        display_name: display_name.to_owned(),
        email: EmailAddress::parse(email).unwrap(),
        auth: AccountAuth::OAuth2(OAuthConfig {
            client_id: OAuthClientId::parse("123.apps.googleusercontent.com").unwrap(),
            auth_url: HttpsUrl::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap(),
            token_url: HttpsUrl::parse("https://oauth2.googleapis.com/token").unwrap(),
            scopes: OAuthScopes::new(vec!["https://mail.google.com/".to_owned()]).unwrap(),
        }),
        incoming: IncomingServerConfig {
            protocol: IncomingProtocol::Imap,
            endpoint: ServerEndpoint::new("imap.gmail.com", port(993)).unwrap(),
            security: ConnectionSecurity::Tls,
            username: email.to_owned(),
            credential: CredentialRef::new("axomail/account/incoming"),
        },
        outgoing: OutgoingServerConfig {
            endpoint: ServerEndpoint::new("smtp.gmail.com", port(465)).unwrap(),
            security: ConnectionSecurity::Tls,
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

/// Builds a message fixture with a custom sender and preview so search
/// tests can target subject, sender, and body text independently.
pub fn search_message(
    mailbox: axomail_core::MailboxId,
    subject: &str,
    sender_name: &str,
    sender_email: &str,
    preview: &str,
    date_unix: Option<i64>,
) -> NewMessage {
    let from = Address {
        display_name: Some(sender_name.to_owned()),
        email: EmailAddress::parse(sender_email).unwrap(),
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
        preview: Some(preview.to_owned()),
        has_raw: false,
    }
}

/// Builds an IMAP [`RemoteEnvelope`] fixture for the ingest path.
pub fn remote_envelope(uid: u32, uid_validity: u32, subject: &str) -> RemoteEnvelope {
    let from = Address {
        display_name: Some("Grace Hopper".to_owned()),
        email: EmailAddress::parse("grace@example.org").unwrap(),
    };
    RemoteEnvelope {
        remote_id: RemoteMessageId::Imap { uid, uid_validity },
        subject: Some(subject.to_owned()),
        from: vec![from],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        date: Some(UtcTimestamp::from_unix_seconds(uid.into())),
        flags: MessageFlags::default(),
        size_bytes: Some(1024),
        threading: None,
    }
}

/// Builds a POP3 [`RemoteEnvelope`] carrying explicit threading identity,
/// the shape a full-header scan produces, for exercising the ingest → read
/// round-trip of the migration-0006 columns.
pub fn pop3_envelope_with_threading(
    uidl: &str,
    subject: &str,
    threading: ThreadingHeaders,
) -> RemoteEnvelope {
    RemoteEnvelope {
        remote_id: RemoteMessageId::Pop3 {
            uidl: uidl.to_owned(),
        },
        subject: Some(subject.to_owned()),
        from: vec![Address::from_email(
            EmailAddress::parse("grace@example.org").unwrap(),
        )],
        to: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(1_700_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(512),
        threading: Some(threading),
    }
}
