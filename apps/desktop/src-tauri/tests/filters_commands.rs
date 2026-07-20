//! Integration tests for the message-filter commands: CRUD + validation via
//! the DTO layer, the per-account run-on-arrival toggle, and a manual run over
//! a folder that relocates a matching message and reports the outcome.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, MailboxId, MailboxName, MailboxRole, MessageFlags,
    RemoteMessageId, UtcTimestamp,
};
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::filters::{
    create_filter_impl, delete_filter_impl, get_account_filter_settings_impl, list_filters_impl,
    reorder_filters_impl, run_filters_on_folder_impl, set_account_filter_arrival_impl,
    set_filter_enabled_impl, update_filter_impl,
};
use axomail_desktop_lib::dto::{FilterActionDto, FilterConditionDto, FilterInputDto};
use axomail_storage::{
    ActionKind, ConditionField, ConditionOp, MatchType, NewMailbox, NewMessage, Store,
};
use common::provider::stub_engine;
use common::{secret_stores, valid_input};

async fn setup() -> (Store, i64, Vec<axomail_desktop_lib::dto::MailboxSummaryDto>) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    (store, summary.id, summary.mailboxes)
}

fn condition(field: ConditionField, op: ConditionOp, value: &str) -> FilterConditionDto {
    FilterConditionDto {
        field,
        op,
        value: value.to_owned(),
    }
}

fn filter_input(account_id: Option<i64>, name: &str, action: FilterActionDto) -> FilterInputDto {
    FilterInputDto {
        account_id,
        name: name.to_owned(),
        enabled: true,
        match_type: MatchType::All,
        stop_on_match: false,
        conditions: vec![condition(
            ConditionField::Subject,
            ConditionOp::Contains,
            "invoice",
        )],
        actions: vec![action],
    }
}

fn star_action() -> FilterActionDto {
    FilterActionDto {
        kind: ActionKind::Star,
        param: String::new(),
    }
}

#[tokio::test]
async fn crud_and_reorder_through_the_command_layer() {
    let (store, account, _) = setup().await;

    let a = create_filter_impl(&store, filter_input(Some(account), "A", star_action()))
        .await
        .unwrap();
    let b = create_filter_impl(&store, filter_input(Some(account), "B", star_action()))
        .await
        .unwrap();
    assert_eq!(a.conditions.len(), 1);

    let listed = list_filters_impl(&store, Some(account)).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name, "A");

    // Update replaces content.
    let mut edit = filter_input(Some(account), "A renamed", star_action());
    edit.enabled = false;
    let updated = update_filter_impl(&store, a.id, edit).await.unwrap();
    assert_eq!(updated.name, "A renamed");
    assert!(!updated.enabled);

    set_filter_enabled_impl(&store, a.id, true).await.unwrap();

    // Reorder B before A.
    reorder_filters_impl(&store, Some(account), vec![b.id, a.id])
        .await
        .unwrap();
    let listed = list_filters_impl(&store, Some(account)).await.unwrap();
    assert_eq!(listed[0].name, "B");

    delete_filter_impl(&store, a.id).await.unwrap();
    assert_eq!(
        list_filters_impl(&store, Some(account))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn validation_rejects_a_filter_with_no_actions() {
    let (store, account, _) = setup().await;
    let mut input = filter_input(Some(account), "Bad", star_action());
    input.actions.clear();
    assert!(create_filter_impl(&store, input).await.is_err());

    let mut bad_forward = filter_input(
        Some(account),
        "Fwd",
        FilterActionDto {
            kind: ActionKind::Forward,
            param: "not-an-email".to_owned(),
        },
    );
    bad_forward.conditions = vec![condition(
        ConditionField::Subject,
        ConditionOp::Contains,
        "x",
    )];
    assert!(create_filter_impl(&store, bad_forward).await.is_err());
}

#[tokio::test]
async fn arrival_setting_defaults_on_and_toggles() {
    let (store, account, _) = setup().await;
    assert!(
        get_account_filter_settings_impl(&store, account)
            .await
            .unwrap()
            .arrival_enabled
    );
    set_account_filter_arrival_impl(&store, account, false)
        .await
        .unwrap();
    assert!(
        !get_account_filter_settings_impl(&store, account)
            .await
            .unwrap()
            .arrival_enabled
    );
}

#[tokio::test]
async fn run_on_folder_relocates_a_matching_message() {
    let (store, account, mailboxes) = setup().await;
    let engine = stub_engine(&store);
    let inbox = mailboxes
        .iter()
        .find(|m| m.role == MailboxRole::Inbox)
        .unwrap()
        .id;
    // IMAP accounts seed only the inbox, so create an Archive folder to move into.
    let target = store
        .mailboxes()
        .upsert(&NewMailbox {
            account: AccountId::new(account),
            name: MailboxName::new("Archive"),
            remote_name: MailboxName::new("Archive"),
            role: MailboxRole::Archive,
            uid_validity: Some(1),
            uid_next: Some(1),
        })
        .await
        .unwrap()
        .id
        .value();

    // An IMAP message in the inbox whose subject matches the filter.
    let message = store
        .messages()
        .insert(&NewMessage {
            mailbox: MailboxId::new(inbox),
            remote_id: Some(RemoteMessageId::Imap {
                uid: 5,
                uid_validity: 1,
            }),
            subject: Some("Your invoice is ready".to_owned()),
            from: vec![Address::from_email(
                EmailAddress::parse("billing@example.org").unwrap(),
            )],
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            date: Some(UtcTimestamp::from_unix_seconds(1)),
            flags: MessageFlags::default(),
            size_bytes: Some(100),
            preview: None,
            has_raw: false,
        })
        .await
        .unwrap()
        .id;

    create_filter_impl(
        &store,
        filter_input(
            Some(account),
            "File invoices",
            FilterActionDto {
                kind: ActionKind::Move,
                param: target.to_string(),
            },
        ),
    )
    .await
    .unwrap();

    let raw_root = tempfile::tempdir().unwrap();
    let outcome = run_filters_on_folder_impl(&store, &engine, raw_root.path(), inbox)
        .await
        .unwrap();
    assert_eq!(outcome.matched, 1);
    assert!(outcome.actions_applied >= 1);

    let envelope = store.messages().get(message).await.unwrap().unwrap();
    assert_eq!(envelope.mailbox.value(), target);
}
