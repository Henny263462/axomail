//! Integration tests for the identity commands and the default-identity seed
//! that account creation performs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::MailError;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::identities::{
    create_identity_impl, delete_identity_impl, list_identities_impl, set_default_identity_impl,
    update_identity_impl,
};
use axomail_desktop_lib::dto::{IdentityDto, IdentityInput};
use axomail_storage::{ArchiveGranularity, ForwardMode, ReplyPosition, SignaturePlacement, Store};
use common::{secret_stores, valid_input};

/// Builds an identity-editor payload with default preferences.
fn identity_input(account_id: i64, email: &str, make_default: bool) -> IdentityInput {
    IdentityInput {
        account_id,
        email: email.to_owned(),
        display_name: String::new(),
        reply_to: None,
        signature_text: None,
        signature_placement: SignaturePlacement::BelowQuote,
        reply_position: ReplyPosition::Below,
        forward_mode: ForwardMode::Inline,
        archive_granularity: ArchiveGranularity::Flat,
        make_default,
        sent_dest: None,
        drafts_dest: None,
        templates_dest: None,
        archive_dest: None,
    }
}

/// The identities belonging to one account.
fn for_account(all: &[IdentityDto], account_id: i64) -> Vec<&IdentityDto> {
    all.iter().filter(|i| i.account_id == account_id).collect()
}

#[tokio::test]
async fn account_creation_seeds_exactly_one_default_identity() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    let all = list_identities_impl(&store).await.unwrap();
    let mine = for_account(&all, summary.id);
    assert_eq!(mine.len(), 1, "one identity is seeded");
    assert!(mine[0].is_default);
    assert_eq!(mine[0].email, "klaus@theferbers.eu");
    assert_eq!(mine[0].display_name, "Work");
}

#[tokio::test]
async fn create_update_default_and_delete_flow() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();

    // Add a second, non-default identity.
    let second = create_identity_impl(
        &store,
        identity_input(summary.id, "sales@theferbers.eu", false),
    )
    .await
    .unwrap();
    assert!(!second.is_default);
    assert_eq!(
        for_account(&list_identities_impl(&store).await.unwrap(), summary.id).len(),
        2
    );

    // Edit it and promote it to default.
    let mut input = identity_input(summary.id, "sales@theferbers.eu", true);
    input.display_name = "Sales".to_owned();
    input.signature_text = Some("Best, Sales".to_owned());
    let updated = update_identity_impl(&store, second.id, input)
        .await
        .unwrap();
    assert_eq!(updated.display_name, "Sales");
    assert_eq!(updated.signature_text.as_deref(), Some("Best, Sales"));
    assert!(updated.is_default);

    // Exactly one default remains, and it is the promoted identity.
    let all = list_identities_impl(&store).await.unwrap();
    let defaults: Vec<_> = for_account(&all, summary.id)
        .into_iter()
        .filter(|i| i.is_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, second.id);

    // Delete the default; the survivor is promoted.
    delete_identity_impl(&store, second.id).await.unwrap();
    let after = list_identities_impl(&store).await.unwrap();
    let mine = for_account(&after, summary.id);
    assert_eq!(mine.len(), 1);
    assert!(mine[0].is_default);
    assert_ne!(mine[0].id, second.id);
}

#[tokio::test]
async fn set_default_via_command_moves_the_flag() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let seeded = for_account(&list_identities_impl(&store).await.unwrap(), summary.id)[0].id;
    let second = create_identity_impl(
        &store,
        identity_input(summary.id, "sales@theferbers.eu", false),
    )
    .await
    .unwrap();

    set_default_identity_impl(&store, second.id).await.unwrap();

    let all = list_identities_impl(&store).await.unwrap();
    assert!(all.iter().find(|i| i.id == second.id).unwrap().is_default);
    assert!(!all.iter().find(|i| i.id == seeded).unwrap().is_default);
}

#[tokio::test]
async fn create_rejects_unknown_account() {
    let store = Store::open_in_memory().await.unwrap();
    let error = create_identity_impl(&store, identity_input(4242, "x@example.org", false))
        .await
        .expect_err("unknown account must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}

#[tokio::test]
async fn create_rejects_an_invalid_address() {
    let store = Store::open_in_memory().await.unwrap();
    let (_c, secrets) = secret_stores();
    let summary = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let error = create_identity_impl(&store, identity_input(summary.id, "not-an-address", false))
        .await
        .expect_err("invalid address must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}

#[tokio::test]
async fn set_default_on_unknown_identity_errors() {
    let store = Store::open_in_memory().await.unwrap();
    let error = set_default_identity_impl(&store, 4242)
        .await
        .expect_err("unknown identity must error");
    assert!(matches!(error, MailError::Configuration { .. }));
}
