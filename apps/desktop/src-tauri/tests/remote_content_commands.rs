//! Integration tests for the `set_remote_content_allowed` command impl.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_core::MailError;
use axomail_desktop_lib::commands::remote_content::set_remote_content_allowed_impl;
use axomail_storage::Store;

#[tokio::test]
async fn set_command_persists_and_clears_a_sender() {
    let store = Store::open_in_memory().await.unwrap();

    set_remote_content_allowed_impl(&store, "Ada@Example.ORG", true)
        .await
        .unwrap();
    // Stored normalized, so a differently-cased lookup matches.
    assert!(
        store
            .remote_content()
            .is_allowed("ada@example.org")
            .await
            .unwrap()
    );

    set_remote_content_allowed_impl(&store, "ada@example.org", false)
        .await
        .unwrap();
    assert!(
        !store
            .remote_content()
            .is_allowed("ada@example.org")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn set_command_rejects_an_empty_sender() {
    let store = Store::open_in_memory().await.unwrap();

    let error = set_remote_content_allowed_impl(&store, "   ", true)
        .await
        .expect_err("an empty sender must be rejected");
    assert!(matches!(error, MailError::Configuration { .. }));
}
