//! Folder-tree tests: discovery reconcile (create / update / rename / orphan),
//! indexed counts, and the server-first CRUD helpers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, Mailbox, MailboxName, MailboxRole, MessageFlags};
use axomail_storage::{DiscoveredMailbox, MailboxCounts, NewMailbox, Store};

use common::{sample_account, sample_message};

/// Opens a store with one IMAP account and its seeded inbox (remote `INBOX`).
async fn store_with_inbox() -> (Store, AccountId, Mailbox) {
    let store = Store::open_in_memory().await.unwrap();
    let account = store
        .accounts()
        .insert(&sample_account("Work", "klaus@theferbers.eu"))
        .await
        .unwrap();
    let inbox = store
        .mailboxes()
        .upsert(&NewMailbox {
            account: account.id,
            name: MailboxName::new("Inbox"),
            remote_name: MailboxName::new("INBOX"),
            role: MailboxRole::Inbox,
            uid_validity: None,
            uid_next: None,
        })
        .await
        .unwrap();
    (store, account.id, inbox)
}

fn discovered(
    remote: &str,
    role: MailboxRole,
    delimiter: Option<&str>,
    subscribed: bool,
) -> DiscoveredMailbox {
    DiscoveredMailbox {
        remote_name: MailboxName::new(remote),
        role,
        delimiter: delimiter.map(str::to_owned),
        subscribed,
    }
}

async fn list(store: &Store, account: AccountId) -> Vec<Mailbox> {
    store.mailboxes().list(account).await.unwrap()
}

fn find<'a>(mailboxes: &'a [Mailbox], remote: &str) -> &'a Mailbox {
    mailboxes
        .iter()
        .find(|mailbox| mailbox.remote_name.as_str() == remote)
        .unwrap_or_else(|| panic!("no mailbox with remote_name {remote}"))
}

#[tokio::test]
async fn reconcile_creates_tree_and_links_parents() {
    let (store, account, inbox) = store_with_inbox().await;
    let folders = vec![
        discovered("INBOX", MailboxRole::Inbox, Some("/"), true),
        discovered("INBOX/Work", MailboxRole::Custom, Some("/"), true),
        discovered("INBOX/Work/2026", MailboxRole::Custom, Some("/"), true),
        discovered("Sent", MailboxRole::Sent, Some("/"), true),
    ];
    let summary = store
        .mailboxes()
        .reconcile(account, &folders)
        .await
        .unwrap();
    assert_eq!(summary.created, 3, "Work, Work/2026, Sent are new");

    let mailboxes = list(&store, account).await;
    // The seeded inbox keeps its id (upsert-by-remote-name, not delete+insert).
    let discovered_inbox = find(&mailboxes, "INBOX");
    assert_eq!(discovered_inbox.id, inbox.id);
    assert_eq!(discovered_inbox.delimiter.as_deref(), Some("/"));

    let work = find(&mailboxes, "INBOX/Work");
    assert_eq!(work.parent, Some(inbox.id));
    let nested = find(&mailboxes, "INBOX/Work/2026");
    assert_eq!(nested.parent, Some(work.id));
    let sent = find(&mailboxes, "Sent");
    assert_eq!(sent.parent, None);
    assert_eq!(sent.role, MailboxRole::Sent);
}

#[tokio::test]
async fn reconcile_updates_role_and_clears_orphan() {
    let (store, account, _) = store_with_inbox().await;
    // First pass: a plain custom folder.
    store
        .mailboxes()
        .reconcile(
            account,
            &[discovered("Spam", MailboxRole::Custom, Some("/"), true)],
        )
        .await
        .unwrap();
    // Second pass: the server now advertises it as Junk (SPECIAL-USE).
    store
        .mailboxes()
        .reconcile(
            account,
            &[discovered("Spam", MailboxRole::Junk, Some("/"), true)],
        )
        .await
        .unwrap();
    let mailboxes = list(&store, account).await;
    assert_eq!(find(&mailboxes, "Spam").role, MailboxRole::Junk);
    assert!(!find(&mailboxes, "Spam").orphaned);
}

#[tokio::test]
async fn reconcile_flags_orphan_with_messages_and_deletes_empty_orphan() {
    let (store, account, _) = store_with_inbox().await;
    store
        .mailboxes()
        .reconcile(
            account,
            &[
                discovered("Keep", MailboxRole::Custom, Some("/"), true),
                discovered("Drop", MailboxRole::Custom, Some("/"), true),
            ],
        )
        .await
        .unwrap();
    let keep = find(&list(&store, account).await, "Keep").id;
    store
        .messages()
        .insert(&sample_message(keep, "held", Some(1)))
        .await
        .unwrap();

    // A pass that no longer lists either folder.
    let summary = store
        .mailboxes()
        .reconcile(
            account,
            &[discovered("INBOX", MailboxRole::Inbox, Some("/"), true)],
        )
        .await
        .unwrap();
    assert_eq!(summary.orphaned, 1, "Keep holds a message");
    assert_eq!(summary.deleted, 1, "Drop was empty");

    let mailboxes = list(&store, account).await;
    let keep_row = find(&mailboxes, "Keep");
    assert!(
        keep_row.orphaned,
        "a folder with local mail is flagged, not dropped"
    );
    assert!(
        !mailboxes.iter().any(|m| m.remote_name.as_str() == "Drop"),
        "the empty orphan is removed"
    );
    // The held message survived the orphaning.
    assert_eq!(
        store.mailboxes().counts(keep_row.id).await.unwrap().total,
        1
    );
}

#[tokio::test]
async fn reconcile_detects_moved_subtree_preserving_id_and_messages() {
    // A moved subtree keeps its *leaf* name (here "Work"), which positively
    // identifies it as the same folder relocated under a new parent. The id and
    // messages must be preserved.
    let (store, account, _) = store_with_inbox().await;
    store
        .mailboxes()
        .reconcile(
            account,
            &[
                discovered("INBOX", MailboxRole::Inbox, Some("/"), true),
                discovered("Sent", MailboxRole::Sent, Some("/"), true),
                discovered("INBOX/Work", MailboxRole::Custom, Some("/"), true),
            ],
        )
        .await
        .unwrap();
    let work_id = find(&list(&store, account).await, "INBOX/Work").id;
    let sent_id = find(&list(&store, account).await, "Sent").id;
    store
        .messages()
        .insert(&sample_message(work_id, "note", Some(1)))
        .await
        .unwrap();

    // The folder is moved on the server: INBOX/Work -> Sent/Work (leaf "Work"
    // preserved). One vanished, one appears, same leaf.
    let summary = store
        .mailboxes()
        .reconcile(
            account,
            &[
                discovered("INBOX", MailboxRole::Inbox, Some("/"), true),
                discovered("Sent", MailboxRole::Sent, Some("/"), true),
                discovered("Sent/Work", MailboxRole::Custom, Some("/"), true),
            ],
        )
        .await
        .unwrap();
    assert_eq!(summary.renamed, 1);
    assert_eq!(summary.orphaned, 0);
    assert_eq!(summary.deleted, 0);

    let mailboxes = list(&store, account).await;
    assert!(
        !mailboxes
            .iter()
            .any(|m| m.remote_name.as_str() == "INBOX/Work")
    );
    let moved = find(&mailboxes, "Sent/Work");
    assert_eq!(moved.id, work_id, "the move preserves the folder id");
    assert_eq!(
        moved.parent,
        Some(sent_id),
        "re-parented under the new path"
    );
    assert_eq!(
        store.mailboxes().counts(moved.id).await.unwrap().total,
        1,
        "the message moved with the relocated folder"
    );
}

#[tokio::test]
async fn reconcile_does_not_merge_unrelated_delete_and_create() {
    // Deleting one folder and creating an unrelated one under the same parent in
    // a single interval must NOT be mistaken for a rename: the leaf names differ,
    // so identity cannot be confirmed. Merging them would reassign the deleted
    // folder's id, messages, and stale sync state to the new folder, and the
    // next sync's UIDVALIDITY mismatch would then wipe the mis-attached cache.
    let (store, account, _) = store_with_inbox().await;
    store
        .mailboxes()
        .reconcile(
            account,
            &[
                discovered("INBOX", MailboxRole::Inbox, Some("/"), true),
                discovered("INBOX/Archive2024", MailboxRole::Custom, Some("/"), true),
            ],
        )
        .await
        .unwrap();
    let archive_id = find(&list(&store, account).await, "INBOX/Archive2024").id;
    store
        .messages()
        .insert(&sample_message(archive_id, "held", Some(1)))
        .await
        .unwrap();

    // Archive2024 vanishes; an unrelated Projects folder appears under INBOX.
    let summary = store
        .mailboxes()
        .reconcile(
            account,
            &[
                discovered("INBOX", MailboxRole::Inbox, Some("/"), true),
                discovered("INBOX/Projects", MailboxRole::Custom, Some("/"), true),
            ],
        )
        .await
        .unwrap();
    assert_eq!(summary.renamed, 0, "leaf changed: never merged");
    assert_eq!(summary.orphaned, 1, "the vanished folder is preserved");
    assert_eq!(summary.created, 1, "the new folder is created fresh");

    let mailboxes = list(&store, account).await;
    let archive = find(&mailboxes, "INBOX/Archive2024");
    assert!(
        archive.orphaned,
        "the deleted folder is orphan-flagged, kept"
    );
    assert_eq!(archive.id, archive_id, "the orphan keeps its own id");
    assert_eq!(
        store.mailboxes().counts(archive.id).await.unwrap().total,
        1,
        "the orphaned folder's message survived (not reassigned or wiped)"
    );
    let projects = find(&mailboxes, "INBOX/Projects");
    assert_ne!(
        projects.id, archive_id,
        "the new folder is a distinct row, not the orphan's id"
    );
    assert_eq!(
        store.mailboxes().counts(projects.id).await.unwrap().total,
        0,
        "the new folder starts empty"
    );
}

#[tokio::test]
async fn counts_report_total_and_unread_per_folder() {
    let (store, account, inbox) = store_with_inbox().await;
    // Two unread, one read.
    for (subject, seen) in [("a", false), ("b", false), ("c", true)] {
        let mut message = sample_message(inbox.id, subject, Some(1));
        message.flags = MessageFlags {
            seen,
            ..MessageFlags::default()
        };
        store.messages().insert(&message).await.unwrap();
    }
    let map = store.mailboxes().counts_for_account(account).await.unwrap();
    let counts = map
        .get(&inbox.id)
        .copied()
        .unwrap_or(MailboxCounts::default());
    assert_eq!(counts.total, 3);
    assert_eq!(counts.unread, 2);

    let single = store.mailboxes().counts(inbox.id).await.unwrap();
    assert_eq!(single, counts);
}

#[tokio::test]
async fn rename_rewrites_descendants_of_a_multibyte_folder() {
    // A German user's folder names carry umlauts, so the path is multi-byte:
    // "Entwürfe" is 8 chars but 9 bytes. SQLite `substr()` counts characters,
    // so the descendant-rewrite offsets must be char counts, not byte lengths —
    // otherwise the child path is never matched and the folder is stranded.
    let (store, account, inbox) = store_with_inbox().await;
    let parent = store
        .mailboxes()
        .create_local(
            account,
            Some(inbox.id),
            &MailboxName::new("Entwürfe"),
            Some("/"),
            MailboxRole::Drafts,
        )
        .await
        .unwrap();
    let child = store
        .mailboxes()
        .create_local(
            account,
            Some(parent.id),
            &MailboxName::new("Entwürfe/Gelöschte"),
            Some("/"),
            MailboxRole::Custom,
        )
        .await
        .unwrap();

    store
        .mailboxes()
        .rename_subtree(parent.id, &MailboxName::new("Drafts"))
        .await
        .unwrap();

    // The child's path must follow the renamed parent, preserving its id.
    let rewritten = store.mailboxes().get(child.id).await.unwrap().unwrap();
    assert_eq!(
        rewritten.remote_name.as_str(),
        "Drafts/Gelöschte",
        "the multi-byte parent's descendant was rewritten under the new prefix"
    );
    assert_eq!(rewritten.parent, Some(parent.id));
    // No stray row kept the stale multi-byte prefix.
    assert!(
        !list(&store, account)
            .await
            .iter()
            .any(|m| m.remote_name.as_str().starts_with("Entwürfe")),
        "no descendant kept the old multi-byte prefix"
    );
}

#[tokio::test]
async fn create_rename_subscribe_delete_roundtrip() {
    let (store, account, inbox) = store_with_inbox().await;
    // Create a child under the inbox.
    let child = store
        .mailboxes()
        .create_local(
            account,
            Some(inbox.id),
            &MailboxName::new("INBOX/Work"),
            Some("/"),
            MailboxRole::Custom,
        )
        .await
        .unwrap();
    assert_eq!(child.parent, Some(inbox.id));
    assert!(child.subscribed);

    // Give it a grandchild, then rename the parent: the descendant path follows.
    store
        .mailboxes()
        .create_local(
            account,
            Some(child.id),
            &MailboxName::new("INBOX/Work/2026"),
            Some("/"),
            MailboxRole::Custom,
        )
        .await
        .unwrap();
    store
        .mailboxes()
        .rename_subtree(child.id, &MailboxName::new("INBOX/Projects"))
        .await
        .unwrap();
    let mailboxes = list(&store, account).await;
    assert!(
        mailboxes
            .iter()
            .any(|m| m.remote_name.as_str() == "INBOX/Projects")
    );
    assert!(
        mailboxes
            .iter()
            .any(|m| m.remote_name.as_str() == "INBOX/Projects/2026"),
        "descendant path was rewritten under the new parent path"
    );

    // Unsubscribe, then delete.
    store
        .mailboxes()
        .set_subscribed(child.id, false)
        .await
        .unwrap();
    assert!(
        !store
            .mailboxes()
            .get(child.id)
            .await
            .unwrap()
            .unwrap()
            .subscribed
    );
    store.mailboxes().delete(child.id).await.unwrap();
    assert!(store.mailboxes().get(child.id).await.unwrap().is_none());
}
