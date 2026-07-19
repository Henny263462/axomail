//! Integration tests for the folder-management commands.
//!
//! A scripted fake provider stands in for the network so the tests can assert
//! the server-first contract: a successful server op updates local storage,
//! and a rejected server op surfaces an error while leaving no phantom folder.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};

use axomail_core::{
    Account, AppendOutcome, FlagDelta, FlagSet, IncomingMailProvider, MailError, MailboxName,
    RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::folders::{
    create_folder_impl, delete_folder_impl, refresh_folders_impl, rename_folder_impl,
    set_folder_subscribed_impl,
};
use axomail_desktop_lib::dto::{CreateFolderInput, RenameFolderInput, SetFolderSubscribedInput};
use axomail_storage::Store;
use axomail_sync::{ProviderFactory, SyncConfig, SyncEngine};
use common::{pop3_account_input, secret_stores, valid_input};

/// Shared, observable state for the fake provider.
#[derive(Default)]
struct FakeState {
    /// When true, every folder op is rejected.
    fail: bool,
    creates: Mutex<Vec<String>>,
}

struct FakeProvider {
    state: Arc<FakeState>,
}

impl FakeProvider {
    fn guard(&self) -> Result<(), MailError> {
        if self.state.fail {
            Err(MailError::Protocol {
                message: "scripted folder-op rejection".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

impl IncomingMailProvider for FakeProvider {
    async fn connect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        Ok(RemoteMailboxState {
            mailboxes: Vec::new(),
        })
    }
    async fn sync(&mut self, _request: SyncRequest) -> Result<SyncOutcome, MailError> {
        Err(MailError::Cancelled)
    }
    async fn fetch_message(&mut self, _id: &RemoteMessageId) -> Result<RawMessage, MailError> {
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
    async fn copy_message(
        &mut self,
        _id: &RemoteMessageId,
        _target: &MailboxName,
    ) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn delete_message(&mut self, _id: &RemoteMessageId) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn expunge_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Err(MailError::Cancelled)
    }
    async fn append_message(
        &mut self,
        _mailbox: &MailboxName,
        _flags: &FlagSet,
        _raw: &[u8],
    ) -> Result<AppendOutcome, MailError> {
        Ok(AppendOutcome::default())
    }
    async fn select_mailbox(&mut self, _mailbox: &MailboxName) -> Result<(), MailError> {
        Ok(())
    }
    async fn create_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        self.guard()?;
        self.state
            .creates
            .lock()
            .unwrap()
            .push(path.as_str().to_owned());
        Ok(())
    }
    async fn rename_mailbox(
        &mut self,
        _from: &MailboxName,
        _to: &MailboxName,
    ) -> Result<(), MailError> {
        self.guard()
    }
    async fn delete_mailbox(&mut self, _path: &MailboxName) -> Result<(), MailError> {
        self.guard()
    }
    async fn set_mailbox_subscribed(
        &mut self,
        _path: &MailboxName,
        _subscribed: bool,
    ) -> Result<(), MailError> {
        self.guard()
    }
    async fn disconnect(&mut self) -> Result<(), MailError> {
        Ok(())
    }
}

struct FakeFactory {
    state: Arc<FakeState>,
}

impl ProviderFactory<FakeProvider> for FakeFactory {
    async fn make(&self, _account: &Account) -> Result<FakeProvider, MailError> {
        Ok(FakeProvider {
            state: Arc::clone(&self.state),
        })
    }
}

/// Opens a store with one IMAP account (seeded inbox) and returns it plus a
/// fresh fake state.
async fn setup() -> (Store, i64, Arc<FakeState>) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    (store, summary.id, Arc::new(FakeState::default()))
}

fn factory(state: &Arc<FakeState>) -> FakeFactory {
    FakeFactory {
        state: Arc::clone(state),
    }
}

async fn folder_names(store: &Store, account: i64) -> Vec<String> {
    store
        .mailboxes()
        .list(axomail_core::AccountId::new(account))
        .await
        .unwrap()
        .into_iter()
        .map(|mailbox| mailbox.remote_name.as_str().to_owned())
        .collect()
}

#[tokio::test]
async fn create_folder_success_records_it_locally() {
    let (store, account, state) = setup().await;
    let dto = create_folder_impl(
        &store,
        &factory(&state),
        CreateFolderInput {
            account_id: account,
            parent_id: None,
            name: "Work".to_owned(),
        },
    )
    .await
    .expect("create should succeed");
    assert_eq!(dto.name, "Work");
    assert!(
        folder_names(&store, account)
            .await
            .contains(&"Work".to_owned())
    );
    assert_eq!(state.creates.lock().unwrap().as_slice(), ["Work"]);
}

#[tokio::test]
async fn create_folder_server_failure_leaves_no_phantom() {
    let (store, account, mut state) = setup().await;
    Arc::get_mut(&mut state).unwrap().fail = true;
    let error = create_folder_impl(
        &store,
        &factory(&state),
        CreateFolderInput {
            account_id: account,
            parent_id: None,
            name: "Ghost".to_owned(),
        },
    )
    .await
    .expect_err("a rejected server op must fail");
    assert!(matches!(error, MailError::Protocol { .. }), "got {error:?}");
    // Server-first: nothing was written locally.
    assert!(
        !folder_names(&store, account)
            .await
            .contains(&"Ghost".to_owned())
    );
}

#[tokio::test]
async fn rename_and_delete_and_subscribe_update_storage() {
    let (store, account, state) = setup().await;
    let created = create_folder_impl(
        &store,
        &factory(&state),
        CreateFolderInput {
            account_id: account,
            parent_id: None,
            name: "Work".to_owned(),
        },
    )
    .await
    .unwrap();

    rename_folder_impl(
        &store,
        &factory(&state),
        RenameFolderInput {
            folder_id: created.id,
            name: "Projects".to_owned(),
        },
    )
    .await
    .unwrap();
    assert!(
        folder_names(&store, account)
            .await
            .contains(&"Projects".to_owned())
    );

    set_folder_subscribed_impl(
        &store,
        &factory(&state),
        SetFolderSubscribedInput {
            folder_id: created.id,
            subscribed: false,
        },
    )
    .await
    .unwrap();
    let mailbox = store
        .mailboxes()
        .get(axomail_core::MailboxId::new(created.id))
        .await
        .unwrap()
        .unwrap();
    assert!(!mailbox.subscribed);

    delete_folder_impl(&store, &factory(&state), created.id)
        .await
        .unwrap();
    assert!(
        !folder_names(&store, account)
            .await
            .contains(&"Projects".to_owned())
    );
}

#[tokio::test]
async fn refresh_folders_gates_on_protocol() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let imap = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let pop3 = create_account_impl(&store, &secrets, pop3_account_input(1100))
        .await
        .unwrap();

    let engine = SyncEngine::start(
        store.clone(),
        FakeFactory {
            state: Arc::new(FakeState::default()),
        },
        SyncConfig::default(),
    );

    // IMAP enqueues a discovery job; POP3 is a no-op; both resolve Ok.
    refresh_folders_impl(&store, &engine, imap.id)
        .await
        .unwrap();
    refresh_folders_impl(&store, &engine, pop3.id)
        .await
        .unwrap();
    // An unknown account is a clean configuration error.
    let error = refresh_folders_impl(&store, &engine, 4242)
        .await
        .expect_err("unknown account");
    assert!(matches!(error, MailError::Configuration { .. }));
}
