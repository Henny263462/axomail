//! Integration tests for the message-template library (KV-backed).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_desktop_lib::commands::templates::{
    delete_template_impl, list_templates_impl, save_template_impl,
};
use axomail_desktop_lib::dto::NewTemplateInput;
use axomail_storage::Store;

fn input(name: &str, subject: &str) -> NewTemplateInput {
    NewTemplateInput {
        name: name.to_owned(),
        subject: subject.to_owned(),
        to: vec!["team@example.org".to_owned()],
        cc: Vec::new(),
        bcc: Vec::new(),
        body_text: "Hello team,\n\n".to_owned(),
    }
}

#[tokio::test]
async fn save_stores_a_template_and_list_returns_it() {
    let store = Store::open_in_memory().await.unwrap();
    assert!(list_templates_impl(&store).await.unwrap().is_empty());

    let saved = save_template_impl(&store, input("Standup", "Daily standup"))
        .await
        .unwrap();
    assert_eq!(saved.name, "Standup");
    assert_eq!(saved.subject, "Daily standup");
    assert_eq!(saved.to, vec!["team@example.org".to_owned()]);
    assert!(saved.id > 0);

    let list = list_templates_impl(&store).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, saved.id);
    assert_eq!(list[0].body_text, "Hello team,\n\n");
}

#[tokio::test]
async fn ids_are_unique_and_delete_removes_only_the_target() {
    let store = Store::open_in_memory().await.unwrap();
    let a = save_template_impl(&store, input("A", "First"))
        .await
        .unwrap();
    let b = save_template_impl(&store, input("B", "Second"))
        .await
        .unwrap();
    assert_ne!(a.id, b.id, "each template gets a fresh id");

    delete_template_impl(&store, a.id).await.unwrap();
    let list = list_templates_impl(&store).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, b.id);

    // Deleting a missing id is a no-op.
    delete_template_impl(&store, 9999).await.unwrap();
    assert_eq!(list_templates_impl(&store).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_blank_name_is_rejected() {
    let store = Store::open_in_memory().await.unwrap();
    let error = save_template_impl(&store, input("   ", "Subject"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        axomail_core::MailError::Configuration { .. }
    ));
}
