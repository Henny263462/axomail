//! Integration test for the startup outbox resume: `queued` (pending) rows
//! are re-sent exactly once, while `failed` rows are left untouched.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::AccountId;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::startup::resume_pending_outbox;
use axomail_storage::{NewOutbox, OutboxState, Store};
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};

use common::{outgoing_account_input, secret_stores};

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

/// Creates an outbox row for `account` with the given subject.
fn new_outbox(account: AccountId, subject: &str) -> NewOutbox {
    NewOutbox {
        account,
        draft_id: None,
        identity_id: None,
        subject: subject.to_owned(),
        body_text: "body".to_owned(),
        to: vec!["bob@example.org".to_owned()],
        cc: Vec::new(),
        bcc: Vec::new(),
        next_attempt_unix: None,
        pgp_protected: false,
    }
}

#[tokio::test]
async fn resume_sends_queued_rows_once_and_leaves_failed_rows() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let summary = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    let account = AccountId::new(summary.id);

    // One row waiting to be sent (pending) and one that previously failed.
    let queued = store
        .outbox()
        .create(&new_outbox(account, "Pending"))
        .await
        .unwrap();
    let failed = store
        .outbox()
        .create(&new_outbox(account, "Failed"))
        .await
        .unwrap();
    store
        .outbox()
        .mark_failed(failed.id, "permanent rejection")
        .await
        .unwrap();

    let attempted = resume_pending_outbox(&store, &secrets, &roots).await;
    assert_eq!(attempted, 1, "only the queued row is resumed");

    // The queued row was sent exactly once.
    let queued_after = store.outbox().get(queued.id).await.unwrap().unwrap();
    assert_eq!(queued_after.state, OutboxState::Sent);
    assert_eq!(queued_after.attempts, 1);
    assert_eq!(server.connection_count(), 1, "one submission only");

    // The failed row is untouched: same state, attempts, and error.
    let failed_after = store.outbox().get(failed.id).await.unwrap().unwrap();
    assert_eq!(failed_after.state, OutboxState::Failed);
    assert_eq!(failed_after.attempts, 0);
    assert_eq!(
        failed_after.last_error.as_deref(),
        Some("permanent rejection")
    );
}
