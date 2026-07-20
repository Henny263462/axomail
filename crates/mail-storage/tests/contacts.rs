//! Integration tests for the contact repository: CRUD with primary-email
//! normalization, prefix autocomplete over name and address, upsert-by-email
//! auto-collection, cascade on delete, and groups (membership + expansion).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_core::EmailAddress;
use axomail_storage::{ContactUpdate, NewContact, NewContactEmail, Store};

fn addr(value: &str) -> EmailAddress {
    EmailAddress::parse(value).unwrap()
}

fn primary(value: &str) -> NewContactEmail {
    NewContactEmail {
        email: addr(value),
        label: "work".to_owned(),
        is_primary: true,
    }
}

async fn store() -> Store {
    Store::open_in_memory().await.unwrap()
}

#[tokio::test]
async fn create_normalizes_primary_and_reads_back_with_emails() {
    let store = store().await;
    let contact = store
        .contacts()
        .create(&NewContact {
            display_name: "Ada Lovelace".to_owned(),
            emails: vec![
                NewContactEmail::plain(addr("ada@home.example.org")),
                primary("ada@work.example.org"),
                // Case-insensitive duplicate of the primary is dropped.
                NewContactEmail::plain(addr("ADA@work.example.org")),
            ],
            note: "  first programmer  ".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(contact.display_name, "Ada Lovelace");
    assert_eq!(contact.note, "first programmer");
    assert_eq!(
        contact.primary_email.as_ref().map(EmailAddress::as_str),
        Some("ada@work.example.org")
    );
    // Two distinct addresses, primary first.
    assert_eq!(contact.emails.len(), 2);
    assert!(contact.emails[0].is_primary);
    assert_eq!(contact.emails[0].email.as_str(), "ada@work.example.org");
    assert_eq!(contact.emails[0].label, "work");
}

#[tokio::test]
async fn blank_name_falls_back_to_local_part() {
    let store = store().await;
    let contact = store
        .contacts()
        .create(&NewContact {
            display_name: "   ".to_owned(),
            emails: vec![NewContactEmail::plain(addr("grace@example.org"))],
            note: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(contact.display_name, "grace");
}

#[tokio::test]
async fn update_replaces_name_note_and_addresses() {
    let store = store().await;
    let id = store
        .contacts()
        .create(&NewContact {
            display_name: "Old".to_owned(),
            emails: vec![NewContactEmail::plain(addr("old@example.org"))],
            note: "old".to_owned(),
        })
        .await
        .unwrap()
        .id;

    let updated = store
        .contacts()
        .update(
            id,
            &ContactUpdate {
                display_name: "New Name".to_owned(),
                emails: vec![primary("new@example.org")],
                note: "new".to_owned(),
            },
        )
        .await
        .unwrap()
        .expect("contact exists");

    assert_eq!(updated.display_name, "New Name");
    assert_eq!(updated.note, "new");
    assert_eq!(updated.emails.len(), 1);
    assert_eq!(updated.emails[0].email.as_str(), "new@example.org");
    // The old address is gone.
    assert!(
        store
            .contacts()
            .find_by_email(&addr("old@example.org"))
            .await
            .unwrap()
            .is_none()
    );

    // Updating a missing contact is Ok(None).
    let ghost = store
        .contacts()
        .update(
            axomail_storage::ContactId::new(9999),
            &ContactUpdate {
                display_name: "x".to_owned(),
                emails: Vec::new(),
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert!(ghost.is_none());
}

#[tokio::test]
async fn delete_cascades_to_addresses() {
    let store = store().await;
    let id = store
        .contacts()
        .create(&NewContact {
            display_name: "Temp".to_owned(),
            emails: vec![NewContactEmail::plain(addr("temp@example.org"))],
            note: String::new(),
        })
        .await
        .unwrap()
        .id;

    assert!(store.contacts().delete(id).await.unwrap());
    assert!(store.contacts().get(id).await.unwrap().is_none());
    // The FK cascade removed the address row too (the email is now free).
    assert!(
        store
            .contacts()
            .find_by_email(&addr("temp@example.org"))
            .await
            .unwrap()
            .is_none()
    );
    // Deleting again is a no-op.
    assert!(!store.contacts().delete(id).await.unwrap());
}

#[tokio::test]
async fn search_matches_name_prefix_email_prefix_and_name_word() {
    let store = store().await;
    store
        .contacts()
        .create(&NewContact {
            display_name: "Ada Lovelace".to_owned(),
            emails: vec![primary("ada@example.org")],
            note: String::new(),
        })
        .await
        .unwrap();
    store
        .contacts()
        .create(&NewContact {
            display_name: "Grace Hopper".to_owned(),
            emails: vec![primary("grace@navy.example.org")],
            note: String::new(),
        })
        .await
        .unwrap();

    // Name prefix.
    let by_name = store.contacts().search_recipients("ada", 10).await.unwrap();
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].email.as_str(), "ada@example.org");

    // Second name word ("lov" -> "Ada Lovelace").
    let by_word = store.contacts().search_recipients("lov", 10).await.unwrap();
    assert_eq!(by_word.len(), 1);
    assert_eq!(by_word[0].display_name, "Ada Lovelace");

    // Email prefix.
    let by_email = store
        .contacts()
        .search_recipients("navy", 10)
        .await
        .unwrap();
    assert!(
        by_email.is_empty(),
        "email match is a prefix, not a substring"
    );
    let by_email = store
        .contacts()
        .search_recipients("grace", 10)
        .await
        .unwrap();
    assert_eq!(by_email.len(), 1);

    // Blank query returns nothing.
    assert!(
        store
            .contacts()
            .search_recipients("  ", 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn search_query_wildcards_are_treated_literally() {
    let store = store().await;
    store
        .contacts()
        .create(&NewContact {
            display_name: "Percent".to_owned(),
            emails: vec![primary("100%@example.org")],
            note: String::new(),
        })
        .await
        .unwrap();
    store
        .contacts()
        .create(&NewContact {
            display_name: "Plain".to_owned(),
            emails: vec![primary("200@example.org")],
            note: String::new(),
        })
        .await
        .unwrap();
    // "100%" matches the literal percent, and the `%` does not act as a
    // wildcard that would also pull in the "200@" contact.
    let hits = store
        .contacts()
        .search_recipients("100%", 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].email.as_str(), "100%@example.org");
}

#[tokio::test]
async fn collect_address_is_idempotent_by_email() {
    let store = store().await;
    // First send collects the address.
    assert!(
        store
            .contacts()
            .collect_address(&addr("bob@example.org"), "Bob")
            .await
            .unwrap()
    );
    // A second send to the same address (any case) collects nothing.
    assert!(
        !store
            .contacts()
            .collect_address(&addr("BOB@example.org"), "Bob B")
            .await
            .unwrap()
    );

    let contacts = store.contacts().list("", 100).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].display_name, "Bob");
    assert_eq!(
        contacts[0].primary_email.as_ref().map(EmailAddress::as_str),
        Some("bob@example.org")
    );
}

/// Regression (finding #19): concurrent auto-collects of the same new address
/// yield exactly one contact — the global unique index + atomic upsert close the
/// check-then-insert race that could otherwise create duplicates.
#[tokio::test]
async fn concurrent_collect_of_the_same_address_yields_one_contact() {
    let store = store().await;
    let email = addr("dana@example.org");

    let mut created = 0u32;
    let mut handles = Vec::new();
    for _ in 0..8 {
        let store = store.clone();
        let email = email.clone();
        handles.push(tokio::spawn(async move {
            store.contacts().collect_address(&email, "Dana").await
        }));
    }
    for handle in handles {
        if handle.await.unwrap().unwrap() {
            created += 1;
        }
    }
    assert_eq!(created, 1, "exactly one racer creates the contact");

    let contacts = store.contacts().list("", 100).await.unwrap();
    assert_eq!(contacts.len(), 1, "no duplicate contacts");
}

#[tokio::test]
async fn collect_address_derives_name_when_blank() {
    let store = store().await;
    store
        .contacts()
        .collect_address(&addr("carol@example.org"), "")
        .await
        .unwrap();
    let found = store
        .contacts()
        .find_by_email(&addr("carol@example.org"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.display_name, "carol");
}

#[tokio::test]
async fn list_filters_by_substring() {
    let store = store().await;
    for (name, email) in [("Ada", "ada@x.org"), ("Grace", "grace@y.org")] {
        store
            .contacts()
            .create(&NewContact {
                display_name: name.to_owned(),
                emails: vec![primary(email)],
                note: String::new(),
            })
            .await
            .unwrap();
    }
    assert_eq!(store.contacts().list("", 100).await.unwrap().len(), 2);
    let filtered = store.contacts().list("grac", 100).await.unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].display_name, "Grace");
    // Substring on the email domain also matches.
    assert_eq!(store.contacts().list("y.org", 100).await.unwrap().len(), 1);
}

#[tokio::test]
async fn groups_membership_and_address_expansion() {
    let store = store().await;
    let ada = store
        .contacts()
        .create(&NewContact {
            display_name: "Ada".to_owned(),
            emails: vec![primary("ada@example.org")],
            note: String::new(),
        })
        .await
        .unwrap()
        .id;
    let grace = store
        .contacts()
        .create(&NewContact {
            display_name: "Grace".to_owned(),
            emails: vec![primary("grace@example.org")],
            note: String::new(),
        })
        .await
        .unwrap()
        .id;
    // A contact with no address is a member but contributes no expansion.
    let nameonly = store
        .contacts()
        .create(&NewContact {
            display_name: "Nomail".to_owned(),
            emails: Vec::new(),
            note: String::new(),
        })
        .await
        .unwrap()
        .id;

    let group = store.contacts().create_group("Pioneers").await.unwrap().id;
    assert!(store.contacts().add_member(group, ada).await.unwrap());
    assert!(store.contacts().add_member(group, grace).await.unwrap());
    assert!(store.contacts().add_member(group, nameonly).await.unwrap());
    // A duplicate membership is a silent no-op.
    assert!(!store.contacts().add_member(group, ada).await.unwrap());

    let listed = store.contacts().list_groups().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].member_count, 3);

    let members = store.contacts().members_of(group).await.unwrap();
    assert_eq!(members.len(), 3);

    // Expansion yields only members with a primary address.
    let addresses = store.contacts().group_addresses(group).await.unwrap();
    let strings: Vec<&str> = addresses.iter().map(EmailAddress::as_str).collect();
    assert_eq!(strings, ["ada@example.org", "grace@example.org"]);

    // Look up a group by name (case-insensitive) for typed-name expansion.
    let by_name = store
        .contacts()
        .find_group_by_name("pioneers")
        .await
        .unwrap();
    assert_eq!(by_name.unwrap().id, group);

    // groups_of reports the contact's memberships.
    assert_eq!(store.contacts().groups_of(ada).await.unwrap(), vec![group]);

    // Removing a member updates the count and expansion.
    assert!(store.contacts().remove_member(group, grace).await.unwrap());
    assert_eq!(
        store.contacts().group_addresses(group).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn deleting_contact_and_group_cascades_membership_both_ways() {
    let store = store().await;
    let ada = store
        .contacts()
        .create(&NewContact {
            display_name: "Ada".to_owned(),
            emails: vec![primary("ada@example.org")],
            note: String::new(),
        })
        .await
        .unwrap()
        .id;
    let group = store.contacts().create_group("Team").await.unwrap().id;
    store.contacts().add_member(group, ada).await.unwrap();

    // Deleting the contact drops the membership but keeps the group.
    store.contacts().delete(ada).await.unwrap();
    let listed = store.contacts().list_groups().await.unwrap();
    assert_eq!(listed[0].member_count, 0);

    // Deleting the group removes it entirely.
    assert!(store.contacts().delete_group(group).await.unwrap());
    assert!(store.contacts().list_groups().await.unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_group_name_is_rejected() {
    let store = store().await;
    store.contacts().create_group("Friends").await.unwrap();
    // Case-insensitive uniqueness.
    assert!(store.contacts().create_group("friends").await.is_err());
    // A blank name is rejected.
    assert!(store.contacts().create_group("   ").await.is_err());
}
