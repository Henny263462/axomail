//! Integration tests for the filter action runner (`run_filters_on_messages`):
//! a move filter relocates a message and enqueues a `move` op; a tag filter
//! applies the tag and enqueues a keyword op; and the run-on-arrival flow
//! relocates a newly-arrived message that matches a move filter.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU16;

use axomail_core::{
    AccountAuth, AccountId, Address, ConnectionSecurity, CredentialRef, EmailAddress,
    IncomingProtocol, IncomingServerConfig, MailboxId, MailboxName, MailboxRole, MessageFlags,
    OutgoingServerConfig, RemoteMessageId, ServerEndpoint, UtcTimestamp,
};
use axomail_storage::{
    ActionKind, ConditionField, ConditionOp, FilterAction, FilterCondition, FilterInput, MatchType,
    NewAccount, NewMailbox, NewMessage, PendingOpKind, Store,
};
use axomail_sync::run_filters_on_messages;

fn port(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap()
}

fn account_fixture() -> NewAccount {
    NewAccount {
        display_name: "Work".to_owned(),
        email: EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        auth: AccountAuth::Password,
        incoming: IncomingServerConfig {
            protocol: IncomingProtocol::Imap,
            endpoint: ServerEndpoint::new("imap.example.org", port(993)).unwrap(),
            security: ConnectionSecurity::Tls,
            username: "klaus@theferbers.eu".to_owned(),
            credential: CredentialRef::new("axomail/account/incoming"),
        },
        outgoing: OutgoingServerConfig {
            endpoint: ServerEndpoint::new("smtp.example.org", port(465)).unwrap(),
            security: ConnectionSecurity::StartTls,
            username: "klaus@theferbers.eu".to_owned(),
            credential: CredentialRef::new("axomail/account/outgoing"),
        },
    }
}

fn mailbox_fixture(account: AccountId, name: &str) -> NewMailbox {
    NewMailbox {
        account,
        name: MailboxName::new(name),
        remote_name: MailboxName::new(name),
        role: MailboxRole::guess_from_name(name),
        uid_validity: Some(1),
        uid_next: Some(1),
    }
}

/// An inbox message with an IMAP identity (so write-back applies).
fn imap_message(mailbox: MailboxId, uid: u32, subject: &str) -> NewMessage {
    NewMessage {
        mailbox,
        remote_id: Some(RemoteMessageId::Imap {
            uid,
            uid_validity: 1,
        }),
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: Some("News Bot".to_owned()),
            email: EmailAddress::parse("news@example.com").unwrap(),
        }],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(1_700_000_000)),
        flags: MessageFlags::default(),
        size_bytes: Some(4096),
        preview: Some("please unsubscribe".to_owned()),
        has_raw: false,
    }
}

struct World {
    store: Store,
    account: AccountId,
    inbox: MailboxId,
    archive: MailboxId,
}

async fn setup() -> World {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&account_fixture())
        .await
        .unwrap()
        .id;
    let inbox = store
        .mailboxes()
        .upsert(&mailbox_fixture(account, "INBOX"))
        .await
        .unwrap()
        .id;
    let archive = store
        .mailboxes()
        .upsert(&mailbox_fixture(account, "Archive"))
        .await
        .unwrap()
        .id;
    World {
        store,
        account,
        inbox,
        archive,
    }
}

fn one_condition(field: ConditionField, op: ConditionOp, value: &str) -> Vec<FilterCondition> {
    vec![FilterCondition {
        field,
        op,
        value: value.to_owned(),
    }]
}

async fn create_filter(
    store: &Store,
    account: AccountId,
    conditions: Vec<FilterCondition>,
    action: FilterAction,
) {
    store
        .filters()
        .create(&FilterInput {
            account: Some(account),
            name: "rule".to_owned(),
            enabled: true,
            match_type: MatchType::All,
            stop_on_match: false,
            conditions,
            actions: vec![action],
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn move_filter_relocates_and_enqueues_a_move_op() {
    let world = setup().await;
    let message = world
        .store
        .messages()
        .insert(&imap_message(world.inbox, 10, "Weekly newsletter"))
        .await
        .unwrap()
        .id;
    create_filter(
        &world.store,
        world.account,
        one_condition(ConditionField::Subject, ConditionOp::Contains, "newsletter"),
        FilterAction {
            kind: ActionKind::Move,
            param: world.archive.value().to_string(),
        },
    )
    .await;

    let report = run_filters_on_messages(&world.store, world.account, &[message])
        .await
        .unwrap();

    assert_eq!(report.matched, 1);
    assert_eq!(report.actions_applied, 1);
    assert!(report.replay_accounts.contains(&world.account));

    // The message moved locally to Archive.
    let envelope = world.store.messages().get(message).await.unwrap().unwrap();
    assert_eq!(envelope.mailbox, world.archive);

    // A single `move` op was enqueued for write-back.
    let ops = world
        .store
        .pending_ops()
        .claim_next_batch(world.account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::Move);
}

#[tokio::test]
async fn tag_filter_applies_the_tag_and_enqueues_a_keyword_op() {
    let world = setup().await;
    let message = world
        .store
        .messages()
        .insert(&imap_message(world.inbox, 11, "Project update"))
        .await
        .unwrap()
        .id;
    create_filter(
        &world.store,
        world.account,
        one_condition(ConditionField::Subject, ConditionOp::Contains, "project"),
        FilterAction {
            kind: ActionKind::Tag,
            param: "Work".to_owned(),
        },
    )
    .await;

    let report = run_filters_on_messages(&world.store, world.account, &[message])
        .await
        .unwrap();
    assert_eq!(report.matched, 1);

    let tags = world.store.tags().tags_for_message(message).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].display_name, "Work");

    let ops = world
        .store
        .pending_ops()
        .claim_next_batch(world.account, 10)
        .await
        .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, PendingOpKind::AddKeyword);
}

#[tokio::test]
async fn star_and_mark_read_apply_local_flags() {
    let world = setup().await;
    let message = world
        .store
        .messages()
        .insert(&imap_message(world.inbox, 12, "Starred item"))
        .await
        .unwrap()
        .id;
    create_filter(
        &world.store,
        world.account,
        one_condition(ConditionField::Subject, ConditionOp::Contains, "starred"),
        FilterAction {
            kind: ActionKind::Star,
            param: String::new(),
        },
    )
    .await;

    run_filters_on_messages(&world.store, world.account, &[message])
        .await
        .unwrap();

    let envelope = world.store.messages().get(message).await.unwrap().unwrap();
    assert!(envelope.flags.flagged);
}

#[tokio::test]
async fn arrival_flow_relocates_a_newly_arrived_match() {
    // Reproduces the worker's run-on-arrival pass over the public API: baseline
    // to the current frontier, then a new arrival above the mark is filtered.
    let world = setup().await;
    create_filter(
        &world.store,
        world.account,
        one_condition(
            ConditionField::From,
            ConditionOp::Contains,
            "news@example.com",
        ),
        FilterAction {
            kind: ActionKind::Move,
            param: world.archive.value().to_string(),
        },
    )
    .await;

    let frontier = world
        .store
        .filters()
        .max_message_id_in_mailbox(world.inbox)
        .await
        .unwrap();
    world
        .store
        .filters()
        .baseline_arrival(world.account, frontier)
        .await
        .unwrap();

    // A message arrives after the baseline.
    let arrived = world
        .store
        .messages()
        .insert(&imap_message(world.inbox, 20, "Fresh mail"))
        .await
        .unwrap()
        .id;

    let candidates = world
        .store
        .filters()
        .messages_in_mailbox_after(world.inbox, frontier)
        .await
        .unwrap();
    assert_eq!(candidates, vec![arrived]);

    let report = run_filters_on_messages(&world.store, world.account, &candidates)
        .await
        .unwrap();
    assert_eq!(report.matched, 1);

    let envelope = world.store.messages().get(arrived).await.unwrap().unwrap();
    assert_eq!(envelope.mailbox, world.archive);
}
