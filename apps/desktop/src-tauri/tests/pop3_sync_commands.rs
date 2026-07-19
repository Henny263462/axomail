//! End-to-end POP3 sync through the composition arm ([`Pop3Sync`]).
//!
//! Drives a real [`Pop3Sync`] against a scripted POP3-over-TLS server: the
//! first sync reports both messages as new, they are ingested, and the second
//! sync — after the arm records their `UIDL`s from the committed rows — reports
//! nothing new. This exercises the known-`UIDL` fill and the crash-safe
//! recording without needing the whole engine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::num::NonZeroU16;

use axomail_core::{
    AccountId, ConnectionSecurity, IncomingMailProvider, MailboxId, MailboxName, MailboxRole,
    MailboxSyncState, ServerEndpoint, SyncRequest,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::pop3_sync::Pop3Sync;
use axomail_pop3::{Pop3Config, Pop3Provider};
use axomail_storage::Store;

use common::pop3_server::{self, Pop3Message};
use common::{pop3_account_input, secret_stores};

/// Builds a sync request for the given inbox mailbox.
fn sync_request(mailbox: MailboxId) -> SyncRequest {
    SyncRequest {
        mailbox,
        remote_name: MailboxName::new("INBOX"),
        state: MailboxSyncState::default(),
        batch_size: 100,
        full_reconcile: false,
    }
}

/// Finds the inbox mailbox id in an account summary.
fn inbox_id(summary: &axomail_desktop_lib::dto::AccountSummaryDto) -> MailboxId {
    let id = summary
        .mailboxes
        .iter()
        .find(|mailbox| mailbox.role == MailboxRole::Inbox)
        .expect("account has an inbox")
        .id;
    MailboxId::new(id)
}

/// Builds a POP3 arm trusting the test server's self-signed certificate.
fn pop3_sync(store: &Store, account: AccountId, port: u16, root_der: &[u8]) -> Pop3Sync {
    let endpoint = ServerEndpoint::new("127.0.0.1".to_owned(), NonZeroU16::new(port).unwrap())
        .expect("endpoint");
    let mut config = Pop3Config::new(endpoint, ConnectionSecurity::Tls, "user", "pass");
    config.extra_root_certs_der = vec![root_der.to_vec()];
    Pop3Sync::new(Pop3Provider::new(config), store.clone(), account)
}

#[tokio::test]
async fn pop3_sync_records_uidls_and_second_sync_skips_them() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();

    let messages = vec![
        Pop3Message::new("UID-0001", "Weekly report", "Alice <alice@example.org>"),
        Pop3Message::new("UID-0002", "Lunch?", "bob@example.org"),
    ];
    let server = pop3_server::spawn(messages, "pass").await;

    let summary = create_account_impl(&store, &secrets, pop3_account_input(server.port))
        .await
        .unwrap();
    let account = AccountId::new(summary.id);
    let inbox = inbox_id(&summary);

    // First sync: both messages are new.
    let mut provider = pop3_sync(&store, account, server.port, &server.root_der);
    provider.connect().await.unwrap();
    let first = provider.sync(sync_request(inbox)).await.unwrap();
    provider.disconnect().await.unwrap();
    assert_eq!(first.new_envelopes.len(), 2, "both messages are new");
    assert!(first.removed.is_empty());

    // Persist the pass; this is what the next sync records UIDLs from.
    let ingest = store.ingest().apply(inbox, &first).await.unwrap();
    assert_eq!(ingest.inserted, 2);

    // Second sync: the arm records the now-committed UIDLs, so nothing is new.
    let mut provider = pop3_sync(&store, account, server.port, &server.root_der);
    provider.connect().await.unwrap();
    let second = provider.sync(sync_request(inbox)).await.unwrap();
    provider.disconnect().await.unwrap();
    assert!(
        second.new_envelopes.is_empty(),
        "known messages must not be re-reported"
    );
    assert!(second.removed.is_empty());

    // The UIDLs are recorded in the de-duplication table.
    let known = store.pop3_uidls().known_uidls(account).await.unwrap();
    assert!(known.contains("UID-0001"));
    assert!(known.contains("UID-0002"));

    // Ingesting the empty second pass inserts nothing (no duplicates).
    let ingest2 = store.ingest().apply(inbox, &second).await.unwrap();
    assert_eq!(ingest2.inserted, 0);
    let page = store.messages().list_page(inbox, None, 10).await.unwrap();
    assert_eq!(page.items.len(), 2, "no duplicate messages after re-sync");
}
