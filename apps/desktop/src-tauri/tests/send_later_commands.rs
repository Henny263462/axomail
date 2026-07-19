//! Integration tests for "send later": the scheduling path on `send_message`,
//! the due/future selection guard the outbox drain honours, and the
//! send-now / cancel-schedule controls.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::AccountId;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::compose::{list_outbox_impl, send_message_impl};
use axomail_desktop_lib::commands::identities::create_identity_impl;
use axomail_desktop_lib::commands::outbox_schedule::{
    cancel_scheduled_send_impl, send_outbox_now_impl,
};
use axomail_desktop_lib::dto::IdentityInput;
use axomail_desktop_lib::startup::drain_due_outbox;
use axomail_storage::{
    ArchiveGranularity, ForwardMode, IdentityId, NewOutbox, OutboxState, ReplyPosition,
    SignaturePlacement, Store,
};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::{compose_input, outgoing_account_input, secret_stores};

/// Current Unix seconds.
fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// An accepting SMTP submission dialog (advertising only `AUTH PLAIN`).
fn smtp_accept_steps() -> Vec<Step> {
    vec![
        Step::send_line("220 axomail.test ESMTP ready"),
        Step::expect_prefix("EHLO"),
        Step::send_line("250-axomail.test greets you"),
        Step::send_line("250-AUTH PLAIN"),
        Step::send_line("250 SIZE 52428800"),
        Step::expect_prefix("AUTH"),
        Step::send_line("235 2.7.0 authentication succeeded"),
        Step::expect_prefix("MAIL FROM"),
        Step::send_line("250 2.1.0 sender ok"),
        Step::expect_prefix("RCPT TO"),
        Step::send_line("250 2.1.5 ok"),
        Step::expect_prefix("DATA"),
        Step::send_line("354 end data with <CR><LF>.<CR><LF>"),
        Step::expect_until("."),
        Step::send_line("250 2.0.0 accepted for delivery"),
        Step::expect_prefix("QUIT"),
        Step::send_line("221 2.0.0 bye"),
        Step::Close,
    ]
}

/// Builds an outbox row for `account`, optionally under an identity and with a
/// schedule.
fn new_outbox(
    account: AccountId,
    subject: &str,
    identity_id: Option<IdentityId>,
    next_attempt_unix: Option<i64>,
) -> NewOutbox {
    NewOutbox {
        account,
        draft_id: None,
        identity_id,
        subject: subject.to_owned(),
        body_text: "body".to_owned(),
        to: vec!["bob@example.org".to_owned()],
        cc: Vec::new(),
        bcc: Vec::new(),
        next_attempt_unix,
    }
}

/// A minimal identity payload for `account`.
fn identity_input(account_id: i64, email: &str, display: &str) -> IdentityInput {
    IdentityInput {
        account_id,
        email: email.to_owned(),
        display_name: display.to_owned(),
        reply_to: None,
        signature_text: None,
        signature_placement: SignaturePlacement::BelowQuote,
        reply_position: ReplyPosition::Below,
        forward_mode: ForwardMode::Inline,
        archive_granularity: ArchiveGranularity::Flat,
        make_default: false,
        sent_dest: None,
        drafts_dest: None,
        templates_dest: None,
        archive_dest: None,
    }
}

#[tokio::test]
async fn send_message_defers_a_future_message_without_sending() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();

    let send_at = now() + 3_600;
    let mut input = compose_input(summary.id, &["bob@example.org"], "Later");
    input.scheduled_at_unix = Some(send_at);
    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;

    assert!(!outcome.sent, "a scheduled message does not leave now");
    assert_eq!(outcome.scheduled_at_unix, Some(send_at));
    assert_eq!(server.connection_count(), 0, "nothing was submitted");

    let items = list_outbox_impl(&store).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, "pending");
    assert_eq!(items[0].scheduled_unix, Some(send_at));

    // The drain leaves the not-yet-due row alone.
    assert_eq!(drain_due_outbox(&store, &secrets, &roots).await, 0);
    assert_eq!(server.connection_count(), 0);
}

#[tokio::test]
async fn drain_sends_due_rows_and_withholds_future_ones() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    let account = AccountId::new(summary.id);

    let due = store
        .outbox()
        .create(&new_outbox(account, "Due", None, Some(now() - 10)))
        .await
        .unwrap();
    let future = store
        .outbox()
        .create(&new_outbox(account, "Future", None, Some(now() + 3_600)))
        .await
        .unwrap();

    let attempted = drain_due_outbox(&store, &secrets, &roots).await;
    assert_eq!(attempted, 1, "only the due row is drained");
    assert_eq!(server.connection_count(), 1);

    assert_eq!(
        store.outbox().get(due.id).await.unwrap().unwrap().state,
        OutboxState::Sent
    );
    assert_eq!(
        store.outbox().get(future.id).await.unwrap().unwrap().state,
        OutboxState::Queued,
        "the future row is still waiting"
    );
}

#[tokio::test]
async fn send_outbox_now_delivers_a_scheduled_row_immediately() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    let record = store
        .outbox()
        .create(&new_outbox(
            AccountId::new(summary.id),
            "Later",
            None,
            Some(now() + 3_600),
        ))
        .await
        .unwrap();

    let outcome = send_outbox_now_impl(&store, &secrets, &roots, record.id.value())
        .await
        .unwrap()
        .outcome;
    assert!(outcome.sent);
    assert_eq!(server.connection_count(), 1);
    let got = store.outbox().get(record.id).await.unwrap().unwrap();
    assert_eq!(got.state, OutboxState::Sent);
    assert!(got.next_attempt_unix.is_none(), "the schedule was cleared");
}

#[tokio::test]
async fn cancel_scheduled_send_returns_the_message_to_a_draft() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, outgoing_account_input(2525))
        .await
        .unwrap();
    let record = store
        .outbox()
        .create(&new_outbox(
            AccountId::new(summary.id),
            "Reconsidered",
            None,
            Some(now() + 3_600),
        ))
        .await
        .unwrap();

    let draft_id = cancel_scheduled_send_impl(&store, record.id.value())
        .await
        .unwrap();

    // The outbox row is gone and a draft carries its content.
    assert!(list_outbox_impl(&store).await.unwrap().is_empty());
    let drafts = store.drafts().list().await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].id.value(), draft_id);
    assert_eq!(drafts[0].subject, "Reconsidered");

    // Cancelling an unknown row is a configuration error.
    assert!(matches!(
        cancel_scheduled_send_impl(&store, 4242).await,
        Err(axomail_core::MailError::Configuration { .. })
    ));
}

#[tokio::test]
async fn cancel_scheduled_send_loses_the_race_to_a_started_send() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, outgoing_account_input(2525))
        .await
        .unwrap();
    let record = store
        .outbox()
        .create(&new_outbox(
            AccountId::new(summary.id),
            "Racing",
            None,
            Some(now() + 3_600),
        ))
        .await
        .unwrap();

    // Simulate the concurrent send winning: it flips the row to `sending`
    // between the user's decision and the cancel's delete.
    assert!(store.outbox().begin_send(record.id).await.unwrap());

    // The cancel now finds no queued row to claim: it errors and creates no
    // draft, and the in-flight row is left untouched.
    let err = cancel_scheduled_send_impl(&store, record.id.value())
        .await
        .unwrap_err();
    assert!(matches!(err, axomail_core::MailError::Configuration { .. }));
    assert!(
        store.drafts().list().await.unwrap().is_empty(),
        "no draft may be created for a message already sending"
    );
    assert_eq!(
        store.outbox().get(record.id).await.unwrap().unwrap().state,
        OutboxState::Sending,
        "the in-flight row must survive the failed cancel"
    );
}

#[tokio::test]
async fn a_deferred_send_uses_its_identity_from_address() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let tls = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &tls)
        .await
        .unwrap();
    let roots = vec![tls.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    let sender = create_identity_impl(
        &store,
        identity_input(summary.id, "sales@theferbers.eu", "Sales"),
    )
    .await
    .unwrap();

    // A due row queued under the sales identity.
    let record = store
        .outbox()
        .create(&new_outbox(
            AccountId::new(summary.id),
            "Promo",
            Some(IdentityId::new(sender.id)),
            Some(now() - 10),
        ))
        .await
        .unwrap();

    assert_eq!(drain_due_outbox(&store, &secrets, &roots).await, 1);
    assert_eq!(
        store.outbox().get(record.id).await.unwrap().unwrap().state,
        OutboxState::Sent
    );
    let transcript = server.transcripts().into_iter().next().unwrap();
    assert!(
        transcript.received_text().contains("sales@theferbers.eu"),
        "the identity's From address should appear in the message"
    );
}
