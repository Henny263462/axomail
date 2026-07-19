//! Integration tests for the tag repository and keyword ingest:
//! CRUD, case-insensitive keyword identity, message associations and the
//! batched page read, FK cascade on account/message/tag delete, usage counts,
//! and the sync path that auto-creates tags from inbound keywords and
//! reconciles them on a flag update.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{
    AccountId, Keyword, MailboxId, MessageFlags, MessageId, RemoteMessageId, SyncOutcome,
};
use axomail_storage::{NewTag, Store, TagColor, TagId, TagUpdate};
use common::{remote_envelope, sample_account, sample_mailbox, sample_message};

/// Opens a store with one account and one mailbox.
async fn setup() -> (Store, AccountId, MailboxId) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap()
        .id;
    let mailbox = store
        .mailboxes()
        .upsert(&sample_mailbox(account, "INBOX"))
        .await
        .unwrap()
        .id;
    (store, account, mailbox)
}

fn keyword(text: &str) -> Keyword {
    Keyword::parse(text).unwrap()
}

async fn new_tag(store: &Store, account: AccountId, kw: &str, name: &str, sort: i64) -> TagId {
    store
        .tags()
        .create(&NewTag {
            account,
            keyword: keyword(kw),
            display_name: name.to_owned(),
            color: Some(TagColor::parse("#123456").unwrap()),
            sort_order: sort,
        })
        .await
        .unwrap()
        .id
}

async fn insert_local_message(store: &Store, mailbox: MailboxId, subject: &str) -> MessageId {
    store
        .messages()
        .insert(&sample_message(mailbox, subject, Some(1)))
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn crud_and_find_by_keyword_are_case_insensitive() {
    let (store, account, _mailbox) = setup().await;
    let id = new_tag(&store, account, "$Junk", "Junk", 0).await;

    // The keyword identity is case-insensitive.
    let found = store
        .tags()
        .find_by_keyword(account, &keyword("$junk"))
        .await
        .unwrap()
        .expect("case-insensitive keyword match");
    assert_eq!(found.id, id);
    assert_eq!(found.display_name, "Junk");

    let updated = store
        .tags()
        .update(
            id,
            &TagUpdate {
                display_name: "Spam".to_owned(),
                color: None,
            },
        )
        .await
        .unwrap()
        .expect("tag exists");
    assert_eq!(updated.display_name, "Spam");
    assert!(updated.color.is_none());

    assert!(store.tags().delete(id).await.unwrap());
    assert!(store.tags().get(id).await.unwrap().is_none());
}

#[tokio::test]
async fn duplicate_keyword_in_one_account_is_rejected() {
    let (store, account, _mailbox) = setup().await;
    new_tag(&store, account, "$Work", "Work", 0).await;
    // Same keyword, different case, in the same account collides on the unique
    // (account_id, keyword COLLATE NOCASE) index.
    let result = store
        .tags()
        .create(&NewTag {
            account,
            keyword: keyword("$work"),
            display_name: "Work again".to_owned(),
            color: None,
            sort_order: 1,
        })
        .await;
    assert!(result.is_err(), "duplicate keyword must be rejected");
}

#[tokio::test]
async fn message_associations_read_and_batch() {
    let (store, account, mailbox) = setup().await;
    let work = new_tag(&store, account, "$Work", "Work", 0).await;
    let todo = new_tag(&store, account, "$Todo", "To Do", 1).await;
    let m1 = insert_local_message(&store, mailbox, "one").await;
    let m2 = insert_local_message(&store, mailbox, "two").await;

    assert!(store.tags().add_message_tag(m1, work).await.unwrap());
    assert!(store.tags().add_message_tag(m1, todo).await.unwrap());
    assert!(store.tags().add_message_tag(m2, work).await.unwrap());
    // A duplicate link is a no-op.
    assert!(!store.tags().add_message_tag(m1, work).await.unwrap());

    let m1_tags = store.tags().tags_for_message(m1).await.unwrap();
    assert_eq!(m1_tags.len(), 2);
    // Ordered by sort_order: Work (0) then To Do (1).
    assert_eq!(m1_tags[0].id, work);
    assert_eq!(m1_tags[1].id, todo);

    let batched = store.tags().tags_for_messages(&[m1, m2]).await.unwrap();
    assert_eq!(batched.get(&m1.value()).unwrap().len(), 2);
    assert_eq!(batched.get(&m2.value()).unwrap().len(), 1);
    // An empty id slice never touches the database.
    assert!(
        store
            .tags()
            .tags_for_messages(&[])
            .await
            .unwrap()
            .is_empty()
    );

    let for_work = store.tags().messages_for_tag(work, 50).await.unwrap();
    assert_eq!(for_work.len(), 2);

    assert!(store.tags().remove_message_tag(m1, todo).await.unwrap());
    assert_eq!(store.tags().tags_for_message(m1).await.unwrap().len(), 1);
}

#[tokio::test]
async fn usage_counts_reflect_links() {
    let (store, account, mailbox) = setup().await;
    let work = new_tag(&store, account, "$Work", "Work", 0).await;
    new_tag(&store, account, "$Idle", "Idle", 1).await;
    let m1 = insert_local_message(&store, mailbox, "one").await;
    store.tags().add_message_tag(m1, work).await.unwrap();

    assert_eq!(store.tags().usage_count(work).await.unwrap(), 1);
    let with_usage = store.tags().list_with_usage(account).await.unwrap();
    let work_usage = with_usage.iter().find(|(t, _)| t.id == work).unwrap().1;
    let idle_usage = with_usage
        .iter()
        .find(|(t, _)| t.display_name == "Idle")
        .unwrap()
        .1;
    assert_eq!(work_usage, 1);
    assert_eq!(idle_usage, 0);

    let all = store.tags().list_all_with_usage().await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn cascade_on_message_and_account_delete() {
    let (store, account, mailbox) = setup().await;
    // Insert an IMAP message via the sync path so it can be removed the same
    // way (the message repo has no public delete; the sync `removed` list is
    // how a message row — and its tag associations — go away).
    let base = SyncOutcome {
        uid_validity: Some(100),
        highest_uid: Some(1),
        has_more: false,
        new_envelopes: vec![remote_envelope(1, 100, "Hello")],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox, &base).await.unwrap();
    let message = first_message(&store, mailbox).await;
    let work = new_tag(&store, account, "$Work", "Work", 0).await;
    store.tags().add_message_tag(message, work).await.unwrap();
    assert_eq!(store.tags().usage_count(work).await.unwrap(), 1);

    // Removing the message on the server drops its associations but keeps the
    // tag (the FK cascade is on `message_tags`, not on `tags`).
    let remove = SyncOutcome {
        uid_validity: Some(100),
        highest_uid: Some(1),
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: vec![RemoteMessageId::Imap {
            uid: 1,
            uid_validity: 100,
        }],
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox, &remove).await.unwrap();
    assert_eq!(store.tags().usage_count(work).await.unwrap(), 0);
    assert!(store.tags().get(work).await.unwrap().is_some());

    // Deleting the account cascades the tag away.
    assert!(store.accounts().delete(account).await.unwrap());
    assert!(store.tags().get(work).await.unwrap().is_none());
}

#[tokio::test]
async fn deleting_a_tag_cascades_its_associations() {
    let (store, account, mailbox) = setup().await;
    let work = new_tag(&store, account, "$Work", "Work", 0).await;
    let m1 = insert_local_message(&store, mailbox, "one").await;
    store.tags().add_message_tag(m1, work).await.unwrap();
    assert_eq!(store.tags().tags_for_message(m1).await.unwrap().len(), 1);

    assert!(store.tags().delete(work).await.unwrap());
    assert!(store.tags().tags_for_message(m1).await.unwrap().is_empty());
}

#[tokio::test]
async fn ingest_auto_creates_tag_from_inbound_keyword() {
    let (store, account, mailbox) = setup().await;
    let mut envelope = remote_envelope(1, 100, "Hello");
    envelope.flags.keywords = [keyword("$Work")].into_iter().collect();
    let outcome = SyncOutcome {
        uid_validity: Some(100),
        highest_uid: Some(1),
        has_more: false,
        new_envelopes: vec![envelope],
        flag_updates: Vec::new(),
        removed: Vec::new(),
        present_remote_ids: None,
    };
    store.ingest().apply(mailbox, &outcome).await.unwrap();

    // A tag was auto-created for the unknown keyword, with a derived display
    // name (leading `$` stripped) and no color.
    let tags = store.tags().list(account).await.unwrap();
    let work = tags
        .iter()
        .find(|tag| tag.keyword.as_str() == "$Work")
        .expect("inbound keyword auto-created a tag");
    assert_eq!(work.display_name, "Work");
    assert!(work.color.is_none());

    // The message is linked to it.
    let for_work = store.tags().messages_for_tag(work.id, 50).await.unwrap();
    assert_eq!(for_work.len(), 1);
}

#[tokio::test]
async fn ingest_reconciles_keywords_on_flag_update() {
    let (store, account, mailbox) = setup().await;
    let mut envelope = remote_envelope(1, 100, "Hello");
    envelope.flags.keywords = [keyword("$Old")].into_iter().collect();
    store
        .ingest()
        .apply(
            mailbox,
            &SyncOutcome {
                uid_validity: Some(100),
                highest_uid: Some(1),
                has_more: false,
                new_envelopes: vec![envelope],
                flag_updates: Vec::new(),
                removed: Vec::new(),
                present_remote_ids: None,
            },
        )
        .await
        .unwrap();
    let message = first_message(&store, mailbox).await;
    assert_eq!(
        store.tags().tags_for_message(message).await.unwrap().len(),
        1
    );

    // A flag update reports the message's complete new keyword set ($New, not
    // $Old): the server is authoritative, so $Old is unlinked and $New linked.
    let flags = MessageFlags {
        seen: true,
        keywords: [keyword("$New")].into_iter().collect(),
        ..MessageFlags::default()
    };
    store
        .ingest()
        .apply(
            mailbox,
            &SyncOutcome {
                uid_validity: Some(100),
                highest_uid: Some(1),
                has_more: false,
                new_envelopes: Vec::new(),
                flag_updates: vec![(
                    RemoteMessageId::Imap {
                        uid: 1,
                        uid_validity: 100,
                    },
                    flags,
                )],
                removed: Vec::new(),
                present_remote_ids: None,
            },
        )
        .await
        .unwrap();

    let tags = store.tags().tags_for_message(message).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].keyword.as_str(), "$New");
    // The dropped keyword's tag still exists in the account (only unlinked).
    assert!(
        store
            .tags()
            .find_by_keyword(account, &keyword("$Old"))
            .await
            .unwrap()
            .is_some()
    );
}

/// Returns the id of the first message in a mailbox.
async fn first_message(store: &Store, mailbox: MailboxId) -> MessageId {
    store
        .messages()
        .list_page(mailbox, None, 10)
        .await
        .unwrap()
        .items
        .first()
        .expect("a message exists")
        .id
}
