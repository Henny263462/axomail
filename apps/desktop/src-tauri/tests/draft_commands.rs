//! Integration tests for the draft commands (no network involved).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::MailError;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::drafts::{delete_draft_impl, list_drafts_impl, save_draft_impl};
use axomail_storage::Store;
use common::{compose_input, secret_stores, valid_input};

#[tokio::test]
async fn draft_save_list_update_delete_round_trip() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let account_id = summary.id;

    // Create.
    let id = save_draft_impl(
        &store,
        compose_input(account_id, &["bob@example.org"], "Draft one"),
        None,
    )
    .await
    .unwrap();

    let drafts = list_drafts_impl(&store).await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].id, id);
    assert_eq!(drafts[0].account_id, account_id);
    assert_eq!(drafts[0].subject, "Draft one");
    assert_eq!(drafts[0].to, vec!["bob@example.org".to_owned()]);

    // Update in place (same id).
    let id2 = save_draft_impl(
        &store,
        compose_input(account_id, &["carol@example.net"], "Draft two"),
        Some(id),
    )
    .await
    .unwrap();
    assert_eq!(id2, id);

    let drafts = list_drafts_impl(&store).await.unwrap();
    assert_eq!(drafts.len(), 1, "update must not create a second draft");
    assert_eq!(drafts[0].subject, "Draft two");
    assert_eq!(drafts[0].to, vec!["carol@example.net".to_owned()]);

    // Delete.
    delete_draft_impl(&store, id).await.unwrap();
    assert!(list_drafts_impl(&store).await.unwrap().is_empty());

    // Deleting an absent draft is a silent no-op.
    delete_draft_impl(&store, id).await.unwrap();
}

#[tokio::test]
async fn save_draft_rejects_updating_a_missing_draft() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let error = save_draft_impl(
        &store,
        compose_input(summary.id, &["bob@example.org"], "ghost"),
        Some(9_999),
    )
    .await
    .expect_err("updating a non-existent draft must fail");
    assert!(matches!(error, MailError::Configuration { .. }));
}
