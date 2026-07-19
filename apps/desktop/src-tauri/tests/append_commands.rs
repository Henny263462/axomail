//! Integration tests for the post-send Sent-copy upload wiring.
//!
//! [`append_sent_copy_impl`] is exercised with a recording fake provider (a
//! successful append adopts the resolved server identity onto the local Sent
//! copy; a failing append is a best-effort no-op that leaves the copy intact),
//! and the send path is checked to be local-only for a POP3 account.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};

use axomail_core::{
    Account, AccountId, AppendOutcome, FlagDelta, FlagSet, IncomingMailProvider, IncomingProtocol,
    MailError, MailboxName, MailboxRole, MessageId, RawMessage, RemoteMailboxState,
    RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_desktop_lib::commands::accounts::{create_account_impl, list_accounts_impl};
use axomail_desktop_lib::commands::compose::{
    SentCopyPlan, append_sent_copy_impl, send_message_impl,
};
use axomail_desktop_lib::dto::CreateAccountInput;
use axomail_storage::Store;
use axomail_sync::ProviderFactory;
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::{
    compose_input, outgoing_account_input, sample_message, secret_stores, seed_standard_folders,
};

/// A shared log of every `(mailbox, flags, raw)` an append was asked to file.
type AppendLog = Arc<Mutex<Vec<(MailboxName, FlagSet, Vec<u8>)>>>;

/// What the recording provider's `append_message` should return.
#[derive(Clone)]
enum AppendReply {
    /// Succeed and report this server identity for adoption.
    Adopt(RemoteMessageId),
    /// Fail the append with a protocol error.
    Fail,
}

/// A recording incoming provider: it captures every append and answers with a
/// configured reply. Only `connect`/`disconnect`/`append_message` are used.
struct RecordingProvider {
    reply: AppendReply,
    appends: AppendLog,
}

impl IncomingMailProvider for RecordingProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Err(MailError::Cancelled)
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        Err(MailError::Cancelled)
    }
    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn store_flags(
        &mut self,
        _id: &RemoteMessageId,
        _delta: &FlagDelta,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn move_message(
        &mut self,
        _id: &RemoteMessageId,
        _target: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn append_message(
        &mut self,
        mailbox: &MailboxName,
        flags: &FlagSet,
        raw: &[u8],
    ) -> Result<AppendOutcome, MailError> {
        self.appends
            .lock()
            .unwrap()
            .push((mailbox.clone(), flags.clone(), raw.to_vec()));
        match &self.reply {
            AppendReply::Adopt(remote_id) => Ok(AppendOutcome {
                remote_id: Some(remote_id.clone()),
            }),
            AppendReply::Fail => Err(MailError::Protocol {
                message: "append rejected".to_owned(),
            }),
        }
    }
    async fn create_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn rename_mailbox(
        &mut self,
        _from: &MailboxName,
        _to: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn set_mailbox_subscribed(
        &mut self,
        _path: &MailboxName,
        _subscribed: bool,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

/// Hands out [`RecordingProvider`]s sharing one reply and append log.
struct RecordingFactory {
    reply: AppendReply,
    appends: AppendLog,
}

impl ProviderFactory<RecordingProvider> for RecordingFactory {
    async fn make(&self, _account: &Account) -> Result<RecordingProvider, MailError> {
        Ok(RecordingProvider {
            reply: self.reply.clone(),
            appends: Arc::clone(&self.appends),
        })
    }
}

/// Seeds a store with an account and a local-only Sent copy, returning the
/// account id and the copy's message id.
async fn store_with_sent_copy() -> (Store, AccountId, MessageId) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let account = create_account_impl(&store, &secrets, common::outgoing_account_input(2525))
        .await
        .unwrap();
    let account_id = AccountId::new(account.id);
    seed_standard_folders(&store, account_id).await;
    let sent = store
        .mailboxes()
        .list(account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .unwrap();
    let copy = store
        .messages()
        .insert(&sample_message(sent.id, "Sent copy", Some(1)))
        .await
        .unwrap();
    (store, account_id, copy.id)
}

#[tokio::test]
async fn append_adopts_the_server_identity_onto_the_local_copy() {
    let (store, account_id, copy_id) = store_with_sent_copy().await;
    let appends = Arc::new(Mutex::new(Vec::new()));
    let adopted = RemoteMessageId::Imap {
        uid: 4321,
        uid_validity: 7,
    };
    let factory = RecordingFactory {
        reply: AppendReply::Adopt(adopted.clone()),
        appends: Arc::clone(&appends),
    };
    let plan = SentCopyPlan {
        account_id,
        local_copy_id: copy_id,
        mailbox_remote: MailboxName::new("Sent"),
        flags: FlagSet::from_system(axomail_core::SystemFlags {
            seen: true,
            ..axomail_core::SystemFlags::default()
        }),
        raw: b"Message-ID: <x@y>\r\n\r\nbody".to_vec(),
    };

    append_sent_copy_impl(&store, &factory, &plan)
        .await
        .expect("append succeeds");

    // The append carried the Sent folder and the \Seen flag. Scope the guard so
    // it is released before the next await.
    {
        let recorded = appends.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, MailboxName::new("Sent"));
        assert!(recorded[0].1.system.seen);
    }

    // The local copy adopted the server identity, so a later sync dedups it.
    let stored = store.messages().get(copy_id).await.unwrap().unwrap();
    assert_eq!(stored.remote_id, Some(adopted));
}

#[tokio::test]
async fn append_failure_leaves_the_local_copy_intact() {
    let (store, account_id, copy_id) = store_with_sent_copy().await;
    let appends = Arc::new(Mutex::new(Vec::new()));
    let factory = RecordingFactory {
        reply: AppendReply::Fail,
        appends: Arc::clone(&appends),
    };
    let plan = SentCopyPlan {
        account_id,
        local_copy_id: copy_id,
        mailbox_remote: MailboxName::new("Sent"),
        flags: FlagSet::default(),
        raw: b"raw".to_vec(),
    };

    // The caller surfaces the error only to log it; the send already succeeded.
    let error = append_sent_copy_impl(&store, &factory, &plan)
        .await
        .expect_err("a rejected append surfaces as an error");
    assert!(matches!(error, MailError::Protocol { .. }));

    // The local Sent copy is untouched: still local-only, still present.
    let stored = store.messages().get(copy_id).await.unwrap().unwrap();
    assert!(stored.remote_id.is_none(), "no server identity was adopted");
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

/// Builds a POP3-incoming account whose outgoing SMTP points at `port`.
fn pop3_incoming_outgoing_smtp(port: u16) -> CreateAccountInput {
    let mut input = outgoing_account_input(port);
    input.incoming.protocol = IncomingProtocol::Pop3;
    input
}

#[tokio::test]
async fn pop3_send_files_locally_without_a_server_append() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let created = create_account_impl(&store, &secrets, pop3_incoming_outgoing_smtp(server.port()))
        .await
        .unwrap();
    let account_id = AccountId::new(created.id);
    seed_standard_folders(&store, account_id).await;
    let summary = list_accounts_impl(&store)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let input = compose_input(summary.id, &["bob@example.org"], "From POP3");
    let submission = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap();

    assert!(submission.outcome.sent, "the send itself succeeds");
    assert!(
        submission.sent_copy.is_none(),
        "a POP3 account has no server folder to append into"
    );

    // The local Sent copy is still filed.
    let sent = store
        .mailboxes()
        .list(account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Sent)
        .unwrap();
    let page = store.messages().list_page(sent.id, None, 10).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].subject.as_deref(), Some("From POP3"));
}

#[tokio::test]
async fn imap_send_produces_a_sent_copy_plan() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_accept_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let created = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    let account_id = AccountId::new(created.id);
    seed_standard_folders(&store, account_id).await;
    let summary = list_accounts_impl(&store)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let input = compose_input(summary.id, &["bob@example.org"], "From IMAP");
    let submission = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap();

    assert!(submission.outcome.sent);
    let plan = submission
        .sent_copy
        .expect("an IMAP account with a Sent folder yields an upload plan");
    assert_eq!(plan.mailbox_remote, MailboxName::new("Sent"));
    assert!(plan.flags.system.seen, "the Sent copy is flagged \\Seen");
    assert!(
        !plan.raw.is_empty(),
        "the rendered bytes are carried for APPEND"
    );
}
