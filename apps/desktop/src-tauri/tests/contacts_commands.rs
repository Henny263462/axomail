//! Integration tests for the contact commands: CRUD + group-membership editor
//! payload, recipient autocomplete (contacts + group expansion), vCard
//! import/export round-trip through the commands, the auto-collect toggle, and
//! auto-collection of recipients on a successful send.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::compose::send_message_impl;
use axomail_desktop_lib::commands::contacts::{
    collect_addresses_enabled, create_contact_group_impl, create_contact_impl, delete_contact_impl,
    export_contacts_vcard_impl, get_contact_impl, import_contacts_vcard_impl,
    list_contact_groups_impl, list_contacts_impl, search_recipients_impl,
    set_collect_addresses_enabled, set_contact_groups_impl, update_contact_impl,
};
use axomail_desktop_lib::dto::{ContactEmailInput, ContactInput};
use axomail_storage::Store;
use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity};
use common::{compose_input, outgoing_account_input, secret_stores, seed_standard_folders};

fn email_input(email: &str, label: &str, primary: bool) -> ContactEmailInput {
    ContactEmailInput {
        email: email.to_owned(),
        label: label.to_owned(),
        is_primary: primary,
    }
}

fn contact_input(name: &str, emails: Vec<ContactEmailInput>) -> ContactInput {
    ContactInput {
        display_name: name.to_owned(),
        emails,
        note: String::new(),
    }
}

#[tokio::test]
async fn crud_round_trips_through_commands() {
    let store = Store::open_in_memory().await.unwrap();
    let created = create_contact_impl(
        &store,
        contact_input(
            "Ada Lovelace",
            vec![
                email_input("ada@home.example.org", "home", false),
                email_input("ada@work.example.org", "work", true),
            ],
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        created.primary_email.as_deref(),
        Some("ada@work.example.org")
    );
    assert_eq!(created.emails.len(), 2);

    let listed = list_contacts_impl(&store, String::new()).await.unwrap();
    assert_eq!(listed.len(), 1);

    let detail = get_contact_impl(&store, created.id).await.unwrap();
    assert_eq!(detail.contact.display_name, "Ada Lovelace");
    assert!(detail.group_ids.is_empty());

    let updated = update_contact_impl(
        &store,
        created.id,
        contact_input("Ada L.", vec![email_input("ada@new.example.org", "", true)]),
    )
    .await
    .unwrap();
    assert_eq!(updated.display_name, "Ada L.");
    assert_eq!(updated.emails.len(), 1);

    delete_contact_impl(&store, created.id).await.unwrap();
    assert!(
        list_contacts_impl(&store, String::new())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn create_rejects_a_malformed_address() {
    let store = Store::open_in_memory().await.unwrap();
    let result = create_contact_impl(
        &store,
        contact_input("Bad", vec![email_input("not-an-email", "", true)]),
    )
    .await;
    assert!(result.is_err(), "a malformed address must be rejected");
}

#[tokio::test]
async fn autocomplete_returns_contacts_and_expands_groups() {
    let store = Store::open_in_memory().await.unwrap();
    let ada = create_contact_impl(
        &store,
        contact_input(
            "Ada Lovelace",
            vec![email_input("ada@example.org", "", true)],
        ),
    )
    .await
    .unwrap();
    let grace = create_contact_impl(
        &store,
        contact_input(
            "Grace Hopper",
            vec![email_input("grace@example.org", "", true)],
        ),
    )
    .await
    .unwrap();

    // A contact match inserts the bare address.
    let hits = search_recipients_impl(&store, "ada".to_owned(), None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "contact");
    assert_eq!(hits[0].value, "ada@example.org");

    // A group whose name matches expands to its members' addresses.
    let group = create_contact_group_impl(&store, "Pioneers".to_owned())
        .await
        .unwrap();
    set_contact_groups_impl(&store, ada.id, vec![group.id])
        .await
        .unwrap();
    set_contact_groups_impl(&store, grace.id, vec![group.id])
        .await
        .unwrap();

    let group_hits = search_recipients_impl(&store, "pio".to_owned(), None)
        .await
        .unwrap();
    assert_eq!(group_hits.len(), 1);
    assert_eq!(group_hits[0].kind, "group");
    assert!(group_hits[0].value.contains("ada@example.org"));
    assert!(group_hits[0].value.contains("grace@example.org"));

    // The editor payload reports the contact's group memberships.
    let detail = get_contact_impl(&store, ada.id).await.unwrap();
    assert_eq!(detail.group_ids, vec![group.id]);

    // A blank query yields nothing.
    assert!(
        search_recipients_impl(&store, "  ".to_owned(), None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn set_contact_groups_replaces_membership() {
    let store = Store::open_in_memory().await.unwrap();
    let ada = create_contact_impl(
        &store,
        contact_input("Ada", vec![email_input("ada@example.org", "", true)]),
    )
    .await
    .unwrap();
    let a = create_contact_group_impl(&store, "A".to_owned())
        .await
        .unwrap();
    let b = create_contact_group_impl(&store, "B".to_owned())
        .await
        .unwrap();

    set_contact_groups_impl(&store, ada.id, vec![a.id, b.id])
        .await
        .unwrap();
    assert_eq!(
        get_contact_impl(&store, ada.id)
            .await
            .unwrap()
            .group_ids
            .len(),
        2
    );

    // Replacing with just B drops A.
    set_contact_groups_impl(&store, ada.id, vec![b.id])
        .await
        .unwrap();
    let detail = get_contact_impl(&store, ada.id).await.unwrap();
    assert_eq!(detail.group_ids, vec![b.id]);

    let groups = list_contact_groups_impl(&store).await.unwrap();
    let b_count = groups.iter().find(|g| g.id == b.id).unwrap().member_count;
    assert_eq!(b_count, 1);
}

#[tokio::test]
async fn vcard_export_then_import_round_trips_through_commands() {
    let store = Store::open_in_memory().await.unwrap();
    create_contact_impl(
        &store,
        ContactInput {
            display_name: "Ada Lovelace".to_owned(),
            emails: vec![
                email_input("ada@work.example.org", "work", true),
                email_input("ada@home.example.org", "home", false),
            ],
            note: "note, with; specials".to_owned(),
        },
    )
    .await
    .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.vcf");
    let path_str = path.to_string_lossy().into_owned();

    let exported = export_contacts_vcard_impl(&store, path_str.clone(), Some("4.0".to_owned()))
        .await
        .unwrap();
    assert_eq!(exported, 1);

    // Import into a fresh store recreates the contact.
    let fresh = Store::open_in_memory().await.unwrap();
    let outcome = import_contacts_vcard_impl(&fresh, path_str.clone())
        .await
        .unwrap();
    assert_eq!(outcome.imported, 1);
    assert_eq!(outcome.skipped, 0);
    let contacts = list_contacts_impl(&fresh, String::new()).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].display_name, "Ada Lovelace");
    assert_eq!(contacts[0].note, "note, with; specials");
    assert_eq!(contacts[0].emails.len(), 2);

    // Re-importing the same file skips the now-known contact.
    let again = import_contacts_vcard_impl(&fresh, path_str).await.unwrap();
    assert_eq!(again.imported, 0);
    assert_eq!(again.skipped, 1);
}

#[tokio::test]
async fn sending_collects_recipients_when_enabled() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_ok_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let created = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    seed_standard_folders(&store, axomail_core::AccountId::new(created.id)).await;

    let input = compose_input(created.id, &["newperson@example.org"], "Hi");
    let outcome = send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap()
        .outcome;
    assert!(outcome.sent);

    // The recipient was auto-collected into the address book.
    let contacts = list_contacts_impl(&store, String::new()).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(
        contacts[0].primary_email.as_deref(),
        Some("newperson@example.org")
    );
    // The derived name is the address's local part.
    assert_eq!(contacts[0].display_name, "newperson");
}

#[tokio::test]
async fn sending_does_not_collect_when_disabled() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(smtp_ok_steps(), &identity)
        .await
        .unwrap();
    let roots = vec![identity.root_der().to_vec()];

    let created = create_account_impl(&store, &secrets, outgoing_account_input(server.port()))
        .await
        .unwrap();
    seed_standard_folders(&store, axomail_core::AccountId::new(created.id)).await;

    // Toggle auto-collection off.
    set_collect_addresses_enabled(&store, false).await.unwrap();
    assert!(!collect_addresses_enabled(&store).await.unwrap());

    let input = compose_input(created.id, &["ignored@example.org"], "Hi");
    send_message_impl(&store, &secrets, &roots, input, false)
        .await
        .unwrap();
    assert!(
        list_contacts_impl(&store, String::new())
            .await
            .unwrap()
            .is_empty()
    );
}

/// A scripted SMTP dialog that accepts one message end to end.
fn smtp_ok_steps() -> Vec<Step> {
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
