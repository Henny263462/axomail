//! Integration tests for the advanced-search runner and the saved-search
//! repository: every facet against a seeded store, all/any combination, the
//! three folder scopes, sort order, CRUD, running a saved search, and graceful
//! behaviour when a folder-scoped search's folder is deleted.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Address, EmailAddress, Keyword, MailboxId, MailboxName, MailboxRole, MessageFlags,
    MessageId, UtcTimestamp,
};
use axomail_storage::{
    ConditionOp, MatchType, NewTag, SavedSearchId, SavedSearchInput, SearchCondition, SearchField,
    SearchQuery, SearchScope, SearchSort, Store,
};
use common::{sample_account, sample_mailbox};

/// Inserts a message with explicit sender, preview, date, and read/starred
/// flags so a facet can be targeted independently.
#[allow(clippy::too_many_arguments)]
async fn seed_message(
    store: &Store,
    mailbox: MailboxId,
    subject: &str,
    sender_name: &str,
    sender_email: &str,
    preview: &str,
    date_unix: i64,
    seen: bool,
    flagged: bool,
) -> MessageId {
    let flags = MessageFlags {
        seen,
        flagged,
        ..MessageFlags::default()
    };
    let new = axomail_storage::NewMessage {
        mailbox,
        remote_id: None,
        subject: Some(subject.to_owned()),
        from: vec![Address {
            display_name: Some(sender_name.to_owned()),
            email: EmailAddress::parse(sender_email).unwrap(),
        }],
        to: vec![Address::from_email(
            EmailAddress::parse("klaus@theferbers.eu").unwrap(),
        )],
        cc: Vec::new(),
        bcc: Vec::new(),
        date: Some(UtcTimestamp::from_unix_seconds(date_unix)),
        flags,
        size_bytes: Some(2048),
        preview: Some(preview.to_owned()),
        has_raw: false,
    };
    store.messages().insert(&new).await.unwrap().id
}

fn condition(field: SearchField, op: ConditionOp, value: &str) -> SearchCondition {
    SearchCondition {
        field,
        op,
        value: value.to_owned(),
    }
}

fn query(
    match_type: MatchType,
    conditions: Vec<SearchCondition>,
    scope: SearchScope,
) -> SearchQuery {
    SearchQuery {
        match_type,
        conditions,
        scope,
        sort: SearchSort::DateDesc,
    }
}

/// Opens a store with one account and its INBOX.
async fn store_with_inbox() -> (Store, AccountId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Personal", "klaus@theferbers.eu"))
        .await
        .unwrap()
        .id;
    let inbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account, "INBOX"))
        .await
        .unwrap()
        .id;
    (store, account, inbox)
}

async fn run_ids(store: &Store, query: &SearchQuery) -> Vec<i64> {
    store
        .saved_searches()
        .run(query, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.message_id.value())
        .collect()
}

#[tokio::test]
async fn each_facet_matches_the_intended_message() {
    let (store, account, inbox) = store_with_inbox().await;
    let invoice = seed_message(
        &store,
        inbox,
        "Quarterly invoice",
        "Ada Lovelace",
        "ada@example.org",
        "please pay the attached invoice soon",
        2_000,
        false,
        true,
    )
    .await;
    let lunch = seed_message(
        &store,
        inbox,
        "Lunch plans",
        "Bob Stone",
        "bob@example.com",
        "let us meet for lunch on friday",
        1_000,
        true,
        false,
    )
    .await;

    // A tag on the invoice only.
    let tag = store
        .tags()
        .create(&NewTag {
            account,
            keyword: Keyword::parse("$work").unwrap(),
            display_name: "Work".to_owned(),
            color: None,
            sort_order: 0,
        })
        .await
        .unwrap()
        .id;
    store.tags().add_message_tag(invoice, tag).await.unwrap();

    let all = SearchScope::AllFolders;
    // Subject.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::Subject,
                    ConditionOp::Contains,
                    "invoice"
                )],
                all.clone()
            )
        )
        .await,
        vec![invoice.value()],
    );
    // From (matches display name or address).
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::From,
                    ConditionOp::Contains,
                    "ada@example.org"
                )],
                all.clone()
            )
        )
        .await,
        vec![invoice.value()],
    );
    // Body (from the preview text).
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::Body,
                    ConditionOp::Contains,
                    "friday"
                )],
                all.clone()
            )
        )
        .await,
        vec![lunch.value()],
    );
    // Tag.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(SearchField::Tag, ConditionOp::Is, "work")],
                all.clone()
            )
        )
        .await,
        vec![invoice.value()],
    );
    // Read flag: only the lunch message is read.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(SearchField::Read, ConditionOp::Is, "true")],
                all.clone()
            )
        )
        .await,
        vec![lunch.value()],
    );
    // Starred: only the invoice is starred.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(SearchField::Starred, ConditionOp::Is, "true")],
                all.clone()
            )
        )
        .await,
        vec![invoice.value()],
    );
    // Date greater-than (invoice is newer).
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::Date,
                    ConditionOp::GreaterThan,
                    "1500"
                )],
                all.clone()
            )
        )
        .await,
        vec![invoice.value()],
    );
    // Folder name.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(SearchField::Folder, ConditionOp::Is, "inbox")],
                all.clone()
            )
        )
        .await
        .len(),
        2,
    );
    // Account name/email.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::Account,
                    ConditionOp::Contains,
                    "personal"
                )],
                all.clone()
            )
        )
        .await
        .len(),
        2,
    );
    // has_attachment: none seeded, so `is false` matches both, `is true` none.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::HasAttachment,
                    ConditionOp::Is,
                    "false"
                )],
                all.clone()
            )
        )
        .await
        .len(),
        2,
    );
    assert!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![condition(
                    SearchField::HasAttachment,
                    ConditionOp::Is,
                    "true"
                )],
                all
            )
        )
        .await
        .is_empty(),
    );
}

#[tokio::test]
async fn all_and_any_combine_conditions() {
    let (store, _account, inbox) = store_with_inbox().await;
    let invoice = seed_message(
        &store,
        inbox,
        "Invoice",
        "Ada",
        "ada@example.org",
        "body",
        2_000,
        false,
        true,
    )
    .await;
    let _receipt = seed_message(
        &store,
        inbox,
        "Receipt",
        "Ada",
        "ada@example.org",
        "body",
        1_000,
        false,
        false,
    )
    .await;

    // ALL: subject contains "invoice" AND starred -> only the invoice.
    let all = query(
        MatchType::All,
        vec![
            condition(SearchField::Subject, ConditionOp::Contains, "invoice"),
            condition(SearchField::Starred, ConditionOp::Is, "true"),
        ],
        SearchScope::AllFolders,
    );
    assert_eq!(run_ids(&store, &all).await, vec![invoice.value()]);

    // ANY: subject contains "receipt" OR starred -> both (receipt by subject,
    // invoice by star), newest first.
    let any = query(
        MatchType::Any,
        vec![
            condition(SearchField::Subject, ConditionOp::Contains, "receipt"),
            condition(SearchField::Starred, ConditionOp::Is, "true"),
        ],
        SearchScope::AllFolders,
    );
    assert_eq!(run_ids(&store, &any).await.len(), 2);
}

#[tokio::test]
async fn scope_narrows_to_folder_subtree_or_all() {
    let (store, account, inbox) = store_with_inbox().await;
    // A child folder under INBOX and a grandchild under it.
    let projects = store
        .mailboxes()
        .create_local(
            account,
            Some(inbox),
            &MailboxName::new("INBOX/Projects"),
            Some("/"),
            MailboxRole::Custom,
        )
        .await
        .unwrap()
        .id;
    // A sibling top-level folder, unrelated to INBOX.
    let archive = store
        .mailboxes()
        .upsert(&sample_mailbox(account, "Archive"))
        .await
        .unwrap()
        .id;

    let in_inbox = seed_message(&store, inbox, "A", "x", "x@e.org", "b", 3_000, false, false).await;
    let in_child = seed_message(
        &store, projects, "B", "x", "x@e.org", "b", 2_000, false, false,
    )
    .await;
    let in_archive = seed_message(
        &store, archive, "C", "x", "x@e.org", "b", 1_000, false, false,
    )
    .await;

    // Just the folder.
    assert_eq!(
        run_ids(
            &store,
            &query(
                MatchType::All,
                vec![],
                SearchScope::Folder { mailbox_id: inbox }
            )
        )
        .await,
        vec![in_inbox.value()],
    );
    // Folder + subtree: INBOX and its child, not the sibling Archive.
    let recursive = run_ids(
        &store,
        &query(
            MatchType::All,
            vec![],
            SearchScope::FolderRecursive { mailbox_id: inbox },
        ),
    )
    .await;
    assert_eq!(recursive, vec![in_inbox.value(), in_child.value()]);
    assert!(!recursive.contains(&in_archive.value()));
    // All folders: everything.
    assert_eq!(
        run_ids(
            &store,
            &query(MatchType::All, vec![], SearchScope::AllFolders)
        )
        .await
        .len(),
        3,
    );
}

#[tokio::test]
async fn sort_orders_results() {
    let (store, _account, inbox) = store_with_inbox().await;
    let zed = seed_message(
        &store, inbox, "Zebra", "x", "x@e.org", "b", 3_000, false, false,
    )
    .await;
    let alpha = seed_message(
        &store, inbox, "Apple", "x", "x@e.org", "b", 1_000, false, false,
    )
    .await;

    let mut q = query(MatchType::All, vec![], SearchScope::AllFolders);
    q.sort = SearchSort::DateDesc;
    assert_eq!(run_ids(&store, &q).await, vec![zed.value(), alpha.value()]);
    q.sort = SearchSort::DateAsc;
    assert_eq!(run_ids(&store, &q).await, vec![alpha.value(), zed.value()]);
    q.sort = SearchSort::SubjectAsc;
    assert_eq!(run_ids(&store, &q).await, vec![alpha.value(), zed.value()]);
    q.sort = SearchSort::SubjectDesc;
    assert_eq!(run_ids(&store, &q).await, vec![zed.value(), alpha.value()]);
}

#[tokio::test]
async fn saved_search_crud_and_run() {
    let (store, _account, inbox) = store_with_inbox().await;
    let invoice = seed_message(
        &store,
        inbox,
        "Invoice",
        "Ada",
        "ada@example.org",
        "b",
        2_000,
        false,
        false,
    )
    .await;
    seed_message(
        &store,
        inbox,
        "Lunch",
        "Bob",
        "bob@example.com",
        "b",
        1_000,
        false,
        false,
    )
    .await;

    let input = SavedSearchInput {
        name: "Invoices".to_owned(),
        query: query(
            MatchType::All,
            vec![condition(
                SearchField::Subject,
                ConditionOp::Contains,
                "invoice",
            )],
            SearchScope::AllFolders,
        ),
    };
    let created = store.saved_searches().create(&input).await.unwrap();
    assert_eq!(created.name, "Invoices");

    // Read back and list.
    let fetched = store
        .saved_searches()
        .get(created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.query, input.query);
    assert_eq!(store.saved_searches().list().await.unwrap().len(), 1);

    // Running it returns the invoice only.
    let hits = store
        .saved_searches()
        .run_saved(created.id, 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message_id, invoice);

    // Update: rename + broaden to match everything.
    let updated = SavedSearchInput {
        name: "Everything".to_owned(),
        query: query(MatchType::All, vec![], SearchScope::AllFolders),
    };
    let after = store
        .saved_searches()
        .update(created.id, &updated)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.name, "Everything");
    assert_eq!(
        store
            .saved_searches()
            .run_saved(created.id, 100)
            .await
            .unwrap()
            .unwrap()
            .len(),
        2,
    );

    // Delete.
    assert!(store.saved_searches().delete(created.id).await.unwrap());
    assert!(
        store
            .saved_searches()
            .get(created.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .saved_searches()
            .run_saved(created.id, 100)
            .await
            .unwrap()
            .is_none()
    );
    // Deleting again is a no-op.
    assert!(!store.saved_searches().delete(created.id).await.unwrap());
}

#[tokio::test]
async fn list_is_alphabetical() {
    let (store, _account, _inbox) = store_with_inbox().await;
    for name in ["banana", "Apple", "cherry"] {
        store
            .saved_searches()
            .create(&SavedSearchInput {
                name: name.to_owned(),
                query: query(MatchType::All, vec![], SearchScope::AllFolders),
            })
            .await
            .unwrap();
    }
    let names: Vec<String> = store
        .saved_searches()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["Apple", "banana", "cherry"]);
}

#[tokio::test]
async fn folder_scoped_search_survives_account_deletion() {
    let (store, account, inbox) = store_with_inbox().await;
    seed_message(
        &store,
        inbox,
        "Invoice",
        "Ada",
        "ada@example.org",
        "b",
        2_000,
        false,
        false,
    )
    .await;

    // A folder-scoped saved search and a global one.
    let scoped = store
        .saved_searches()
        .create(&SavedSearchInput {
            name: "In INBOX".to_owned(),
            query: query(
                MatchType::All,
                vec![],
                SearchScope::Folder { mailbox_id: inbox },
            ),
        })
        .await
        .unwrap();
    let global = store
        .saved_searches()
        .create(&SavedSearchInput {
            name: "Everywhere".to_owned(),
            query: query(MatchType::All, vec![], SearchScope::AllFolders),
        })
        .await
        .unwrap();

    // Deleting the account cascades its mailboxes and messages away.
    assert!(store.accounts().delete(account).await.unwrap());

    // Both saved searches still exist (they are global rows, no FK to the
    // account) and run without error — now over an empty candidate set.
    assert_eq!(store.saved_searches().list().await.unwrap().len(), 2);
    assert!(
        store
            .saved_searches()
            .run_saved(scoped.id, 100)
            .await
            .unwrap()
            .unwrap()
            .is_empty(),
        "a search scoped to a deleted folder resolves to empty, not an error",
    );
    assert!(
        store
            .saved_searches()
            .run_saved(global.id, 100)
            .await
            .unwrap()
            .unwrap()
            .is_empty(),
    );
}

#[tokio::test]
async fn run_saved_is_none_for_unknown_id() {
    let (store, _account, _inbox) = store_with_inbox().await;
    assert!(
        store
            .saved_searches()
            .run_saved(SavedSearchId::new(999), 100)
            .await
            .unwrap()
            .is_none()
    );
}
