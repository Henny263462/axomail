//! Integration tests for FTS5 search and the unified inbox, exercised
//! against in-memory SQLite databases.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, MailboxId, SyncOutcome};
use axomail_storage::{MessageCursor, Store};
use common::{remote_envelope, sample_account, sample_mailbox, search_message};

/// Opens a store with one account and its INBOX, returning the account
/// and mailbox ids.
async fn store_with_inbox() -> (Store, AccountId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account.id, "INBOX"))
        .await
        .unwrap();
    (store, account.id, mailbox.id)
}

#[tokio::test]
async fn fts5_index_is_available() {
    // Opening the store runs migration 0005; if the bundled SQLite lacked
    // FTS5 the `CREATE VIRTUAL TABLE` there would fail and this errors.
    let (store, _account, _mailbox) = store_with_inbox().await;

    // The virtual table exists and a MATCH query runs (empty index → no
    // hits) rather than failing to parse.
    let hits = store.search().search(&[], "anything", 10).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn index_and_search_round_trip() {
    let (store, _account, mailbox) = store_with_inbox().await;
    store
        .messages()
        .insert(&search_message(
            mailbox,
            "Quarterly Report",
            "Ada Lovelace",
            "ada@example.org",
            "the body mentions pineapples in detail",
            Some(100),
        ))
        .await
        .unwrap();
    store
        .messages()
        .insert(&search_message(
            mailbox,
            "Café Meeting notes",
            "René Descartes",
            "rene@example.org",
            "agenda for the strategy session",
            Some(200),
        ))
        .await
        .unwrap();

    let search = store.search();

    // Subject match.
    assert_eq!(search.search(&[], "Quarterly", 10).await.unwrap().len(), 1);
    // Sender name and address match.
    assert_eq!(search.search(&[], "Lovelace", 10).await.unwrap().len(), 1);
    assert_eq!(search.search(&[], "ada", 10).await.unwrap().len(), 1);
    // Body (preview) match.
    let body_hit = search.search(&[], "pineapples", 10).await.unwrap();
    assert_eq!(body_hit.len(), 1);
    // Prefix match on subject and body.
    assert_eq!(search.search(&[], "quar*", 10).await.unwrap().len(), 1);
    assert_eq!(search.search(&[], "pineap*", 10).await.unwrap().len(), 1);
    // Diacritics-insensitive: "cafe" matches "Café", "rene" matches "René".
    assert_eq!(search.search(&[], "cafe", 10).await.unwrap().len(), 1);
    assert_eq!(search.search(&[], "rene", 10).await.unwrap().len(), 1);

    // A snippet is returned with the matched term highlighted.
    let snippet = &body_hit[0].snippet;
    assert!(snippet.contains("pineapples"), "snippet was {snippet:?}");
    assert!(snippet.contains('['), "snippet lacked markers: {snippet:?}");
}

#[tokio::test]
async fn index_message_body_makes_body_searchable() {
    let (store, _account, mailbox) = store_with_inbox().await;
    // Insert without a useful body, as the ingest path does.
    let message = store
        .messages()
        .insert(&search_message(
            mailbox,
            "Logistics",
            "Ada Lovelace",
            "ada@example.org",
            "",
            Some(1),
        ))
        .await
        .unwrap();

    assert!(
        store
            .search()
            .search(&[], "zebra", 10)
            .await
            .unwrap()
            .is_empty()
    );

    // The detail path indexes the extracted body later.
    store
        .search()
        .index_message_body(message.id, "a special zebra crossing the savanna")
        .await
        .unwrap();

    let hits = store.search().search(&[], "zebra", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message_id, message.id);
    // Re-indexing must not lose the already indexed header text.
    assert_eq!(
        store
            .search()
            .search(&[], "Logistics", 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn query_escaping_treats_syntax_as_literals() {
    let (store, _account, mailbox) = store_with_inbox().await;
    store
        .messages()
        .insert(&search_message(
            mailbox,
            "meeting AND agenda (final)",
            "Ada Lovelace",
            "ada@example.org",
            "notes",
            Some(1),
        ))
        .await
        .unwrap();
    let search = store.search();

    // "AND" is searched as a literal token (the subject contains it),
    // not interpreted as the FTS5 boolean operator.
    assert_eq!(search.search(&[], "AND", 10).await.unwrap().len(), 1);
    // Parentheses are stripped by the tokenizer, so "(final)" matches.
    assert_eq!(search.search(&[], "(final)", 10).await.unwrap().len(), 1);
    // "OR" as a literal is absent from the document, so ANDed tokens miss
    // — proving it is not treated as an operator (which would match).
    assert!(
        search
            .search(&[], "meeting OR agenda", 10)
            .await
            .unwrap()
            .is_empty()
    );
    // Asterisk prefix still works alongside the escaping.
    assert_eq!(search.search(&[], "agen*", 10).await.unwrap().len(), 1);
    // A stray double quote must not produce a MATCH syntax error.
    assert!(search.search(&[], "final\"", 10).await.unwrap().len() <= 1);
    // Token-free queries yield an empty result, never an error.
    assert!(search.search(&[], "   ", 10).await.unwrap().is_empty());
    assert!(search.search(&[], "* () -", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn search_can_be_scoped_to_accounts() {
    let store = Store::open_in_memory().await.unwrap();
    let mut mailboxes = Vec::new();
    let mut accounts = Vec::new();
    for index in 0..2 {
        let account = store
            .accounts()
            .insert(&sample_account(
                &format!("Acct {index}"),
                &format!("user{index}@example.org"),
            ))
            .await
            .unwrap();
        let mailbox = store
            .mailboxes()
            .upsert(&sample_mailbox(account.id, "INBOX"))
            .await
            .unwrap();
        store
            .messages()
            .insert(&search_message(
                mailbox.id,
                "shared keyword flamingo",
                "Ada Lovelace",
                "ada@example.org",
                "body",
                Some(1),
            ))
            .await
            .unwrap();
        accounts.push(account.id);
        mailboxes.push(mailbox.id);
    }

    // All accounts (empty filter) see both hits.
    assert_eq!(
        store
            .search()
            .search(&[], "flamingo", 10)
            .await
            .unwrap()
            .len(),
        2
    );
    // Scoped to one account returns only that account's message.
    let scoped = store
        .search()
        .search(&[accounts[0]], "flamingo", 10)
        .await
        .unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].mailbox_id, mailboxes[0]);
}

#[tokio::test]
async fn ingested_envelopes_are_searchable_and_removals_disappear() {
    let (store, _account, mailbox) = store_with_inbox().await;

    let outcome = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(10),
        has_more: false,
        new_envelopes: vec![remote_envelope(10, 1, "Ingested Zephyr project")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
    };
    let summary = store.ingest().apply(mailbox, &outcome).await.unwrap();
    assert_eq!(summary.inserted, 1);

    // Ingest indexed the header text: subject and sender are searchable.
    assert_eq!(
        store
            .search()
            .search(&[], "Zephyr", 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .search()
            .search(&[], "Hopper", 10)
            .await
            .unwrap()
            .len(),
        1
    );

    // Removing the message on the next pass drops it from the index too
    // (content cascade fires the FTS delete trigger).
    let removal = SyncOutcome {
        uid_validity: Some(1),
        highest_uid: Some(10),
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: vec![axomail_core::RemoteMessageId::Imap {
            uid: 10,
            uid_validity: 1,
        }],
    };
    let summary = store.ingest().apply(mailbox, &removal).await.unwrap();
    assert_eq!(summary.removed, 1);
    assert!(
        store
            .search()
            .search(&[], "Zephyr", 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn rebuild_resume_bookkeeping_round_trips() {
    let (store, _account, mailbox) = store_with_inbox().await;
    let search = store.search();

    // No progress recorded yet.
    assert!(search.index_progress(mailbox).await.unwrap().is_none());

    let first = store
        .messages()
        .insert(&search_message(
            mailbox,
            "one",
            "Ada",
            "ada@example.org",
            "b",
            Some(1),
        ))
        .await
        .unwrap();
    let second = store
        .messages()
        .insert(&search_message(
            mailbox,
            "two",
            "Ada",
            "ada@example.org",
            "b",
            Some(2),
        ))
        .await
        .unwrap();

    search.set_index_progress(mailbox, first.id).await.unwrap();
    assert_eq!(
        search.index_progress(mailbox).await.unwrap(),
        Some(first.id)
    );

    // Advancing the cursor overwrites the prior position (upsert).
    search.set_index_progress(mailbox, second.id).await.unwrap();
    assert_eq!(
        search.index_progress(mailbox).await.unwrap(),
        Some(second.id)
    );
}

#[tokio::test]
async fn unified_inbox_paginates_across_accounts_by_date() {
    let store = Store::open_in_memory().await.unwrap();

    // Two accounts, each with an INBOX and a non-inbox folder.
    let mut inbox_by_account = Vec::new();
    for index in 0..2 {
        let account = store
            .accounts()
            .insert(&sample_account(
                &format!("Acct {index}"),
                &format!("user{index}@example.org"),
            ))
            .await
            .unwrap();
        let inbox = store
            .mailboxes()
            .upsert(&sample_mailbox(account.id, "INBOX"))
            .await
            .unwrap();
        let sent = store
            .mailboxes()
            .upsert(&sample_mailbox(account.id, "Sent"))
            .await
            .unwrap();
        // A message in a non-inbox folder must never appear in the view.
        store
            .messages()
            .insert(&search_message(
                sent.id,
                "sent item",
                "Ada",
                "ada@example.org",
                "b",
                Some(999),
            ))
            .await
            .unwrap();
        inbox_by_account.push((account.id, inbox.id));
    }

    // Interleave dates across the two accounts: account 0 gets 10/30/50,
    // account 1 gets 20/40/60.
    let mut expected: Vec<(AccountId, i64)> = Vec::new();
    for (slot, (account_id, inbox_id)) in inbox_by_account.iter().enumerate() {
        for step in 0..3 {
            let date = 10 * (2 * step + slot as i64 + 1);
            store
                .messages()
                .insert(&search_message(
                    *inbox_id,
                    &format!("acct {account_id} date {date}"),
                    "Ada",
                    "ada@example.org",
                    "body",
                    Some(date),
                ))
                .await
                .unwrap();
            expected.push((*account_id, date));
        }
    }
    // Newest first across both accounts.
    expected.sort_by_key(|entry| std::cmp::Reverse(entry.1));

    let mut seen: Vec<(AccountId, i64)> = Vec::new();
    let mut cursor: Option<MessageCursor> = None;
    let mut pages = 0;
    loop {
        let page = store.unified().list_unified_inbox(cursor, 2).await.unwrap();
        pages += 1;
        for item in &page.items {
            seen.push((item.account, item.envelope.date.unwrap().unix_seconds()));
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert!(pages > 2, "expected multiple pages, got {pages}");
    assert_eq!(seen.len(), 6, "sent-folder message must be excluded");
    assert_eq!(seen, expected);
}
