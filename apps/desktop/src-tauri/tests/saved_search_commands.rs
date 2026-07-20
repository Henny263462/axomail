//! Integration tests for the advanced-search and saved-search commands: the
//! ad-hoc run joined with envelopes, saved-search CRUD through the DTO layer,
//! running a saved search, and input validation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::MailboxId;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::saved_search::{
    create_saved_search_impl, delete_saved_search_impl, list_saved_searches_impl,
    run_advanced_search_impl, run_saved_search_impl, update_saved_search_impl,
};
use axomail_desktop_lib::dto::{
    AccountSummaryDto, SavedSearchInputDto, SearchConditionDto, SearchQueryDto, SearchScopeDto,
    SearchScopeKind,
};
use axomail_storage::{ConditionOp, MatchType, SearchField, SearchSort, Store};

use common::{sample_message, secret_stores, valid_input};

fn inbox_id(summary: &AccountSummaryDto) -> MailboxId {
    let id = summary
        .mailboxes
        .iter()
        .find(|mailbox| mailbox.role == axomail_core::MailboxRole::Inbox)
        .expect("account has an inbox")
        .id;
    MailboxId::new(id)
}

fn condition(field: SearchField, op: ConditionOp, value: &str) -> SearchConditionDto {
    SearchConditionDto {
        field,
        op,
        value: value.to_owned(),
    }
}

fn query(conditions: Vec<SearchConditionDto>) -> SearchQueryDto {
    SearchQueryDto {
        match_type: MatchType::All,
        conditions,
        scope: SearchScopeDto {
            kind: SearchScopeKind::AllFolders,
            mailbox_id: None,
        },
        sort: SearchSort::DateDesc,
    }
}

async fn setup() -> (Store, AccountSummaryDto, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let inbox = inbox_id(&summary);
    store
        .messages()
        .insert(&sample_message(inbox, "Quarterly invoice", Some(2_000)))
        .await
        .unwrap();
    store
        .messages()
        .insert(&sample_message(inbox, "Lunch plans", Some(1_000)))
        .await
        .unwrap();
    (store, summary, inbox)
}

#[tokio::test]
async fn advanced_search_joins_envelopes() {
    let (store, summary, inbox) = setup().await;
    let results = run_advanced_search_impl(
        &store,
        query(vec![condition(
            SearchField::Subject,
            ConditionOp::Contains,
            "invoice",
        )]),
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    let hit = &results[0];
    assert_eq!(hit.account_id, summary.id);
    assert_eq!(hit.mailbox_id, inbox.value());
    assert_eq!(hit.subject.as_deref(), Some("Quarterly invoice"));
    assert_eq!(hit.from_name, "Ada Lovelace");
    assert_eq!(hit.date_unix, Some(2_000));
}

#[tokio::test]
async fn saved_search_crud_and_run_through_commands() {
    let (store, _summary, _inbox) = setup().await;

    let created = create_saved_search_impl(
        &store,
        SavedSearchInputDto {
            name: "Invoices".to_owned(),
            query: query(vec![condition(
                SearchField::Subject,
                ConditionOp::Contains,
                "invoice",
            )]),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.name, "Invoices");

    // Listed.
    let listed = list_saved_searches_impl(&store).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);

    // Running it returns the invoice only.
    let hits = run_saved_search_impl(&store, created.id).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject.as_deref(), Some("Quarterly invoice"));

    // Update to match everything, then re-run.
    update_saved_search_impl(
        &store,
        created.id,
        SavedSearchInputDto {
            name: "Everything".to_owned(),
            query: query(vec![]),
        },
    )
    .await
    .unwrap();
    let all = run_saved_search_impl(&store, created.id).await.unwrap();
    assert_eq!(all.len(), 2);

    // Delete.
    delete_saved_search_impl(&store, created.id).await.unwrap();
    assert!(list_saved_searches_impl(&store).await.unwrap().is_empty());
    // Running a deleted saved search resolves to empty, not an error.
    assert!(
        run_saved_search_impl(&store, created.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn invalid_input_is_rejected() {
    let (store, _summary, _inbox) = setup().await;

    // A bad regex.
    let bad_regex = run_advanced_search_impl(
        &store,
        query(vec![condition(
            SearchField::Subject,
            ConditionOp::MatchesRegex,
            "(",
        )]),
    )
    .await;
    assert!(bad_regex.is_err(), "an invalid regex is rejected");

    // A blank name on a saved search.
    let blank = create_saved_search_impl(
        &store,
        SavedSearchInputDto {
            name: "   ".to_owned(),
            query: query(vec![]),
        },
    )
    .await;
    assert!(blank.is_err(), "a blank name is rejected");

    // A folder scope with no folder.
    let no_folder = run_advanced_search_impl(
        &store,
        SearchQueryDto {
            match_type: MatchType::All,
            conditions: vec![],
            scope: SearchScopeDto {
                kind: SearchScopeKind::Folder,
                mailbox_id: None,
            },
            sort: SearchSort::DateDesc,
        },
    )
    .await;
    assert!(no_folder.is_err(), "a folder scope needs a folder");
}
