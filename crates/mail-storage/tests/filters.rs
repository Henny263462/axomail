//! Integration tests for the message-filter engine: the pure evaluator (every
//! op, all/any, stop-on-match, regex, size/date/attachment/tag), the
//! `FilterRepo` CRUD + reorder + scope, the facts loader, and the
//! run-on-arrival state (baseline, toggle, high-water advance, candidate scan).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axomail_core::{AccountId, MailboxId, MessageFlags, UtcTimestamp};
use axomail_storage::{
    ActionKind, ConditionField, ConditionOp, Filter, FilterAction, FilterCondition, FilterId,
    FilterInput, FilterMessageFacts, FilterProgram, MatchType, NewMessage, Store,
    validate_filter_input,
};
use common::{sample_account, sample_mailbox, sample_message};

// ---------------------------------------------------------------------------
// Pure evaluator
// ---------------------------------------------------------------------------

fn cond(field: ConditionField, op: ConditionOp, value: &str) -> FilterCondition {
    FilterCondition {
        field,
        op,
        value: value.to_owned(),
    }
}

fn star_filter(match_type: MatchType, conditions: Vec<FilterCondition>) -> Filter {
    Filter {
        id: FilterId::new(1),
        account: None,
        name: "f".to_owned(),
        enabled: true,
        match_type,
        sort_order: 0,
        stop_on_match: false,
        conditions,
        actions: vec![FilterAction {
            kind: ActionKind::Star,
            param: String::new(),
        }],
    }
}

fn sample_facts() -> FilterMessageFacts {
    FilterMessageFacts {
        subject: "weekly newsletter".to_owned(),
        from: vec!["news bot news@example.com".to_owned()],
        to: vec!["me me@example.org".to_owned()],
        cc: vec!["boss boss@work.example".to_owned()],
        body: "hello world please unsubscribe here".to_owned(),
        tags: vec!["$work".to_owned(), "important".to_owned()],
        has_attachment: true,
        size: Some(2048),
        date: Some(1_700_000_000),
    }
}

fn matches(filters: &[Filter], facts: &FilterMessageFacts) -> bool {
    !FilterProgram::compile(filters).evaluate(facts).is_empty()
}

#[test]
fn every_string_op_matches_as_expected() {
    let facts = sample_facts();
    let cases = [
        (ConditionField::Subject, ConditionOp::Contains, "news", true),
        (
            ConditionField::Subject,
            ConditionOp::Contains,
            "daily",
            false,
        ),
        (
            ConditionField::Subject,
            ConditionOp::Is,
            "weekly newsletter",
            true,
        ),
        (
            ConditionField::Subject,
            ConditionOp::IsNot,
            "weekly newsletter",
            false,
        ),
        (
            ConditionField::Subject,
            ConditionOp::StartsWith,
            "weekly",
            true,
        ),
        (
            ConditionField::Subject,
            ConditionOp::EndsWith,
            "letter",
            true,
        ),
        (
            ConditionField::From,
            ConditionOp::Contains,
            "example.com",
            true,
        ),
        (
            ConditionField::From,
            ConditionOp::NotContains,
            "spam.net",
            true,
        ),
        (
            ConditionField::From,
            ConditionOp::NotContains,
            "example.com",
            false,
        ),
        (ConditionField::Cc, ConditionOp::Contains, "boss", true),
        (
            ConditionField::Body,
            ConditionOp::Contains,
            "unsubscribe",
            true,
        ),
        (ConditionField::Tag, ConditionOp::Is, "important", true),
        (ConditionField::Tag, ConditionOp::Contains, "$work", true),
    ];
    for (field, op, value, expected) in cases {
        assert_eq!(
            matches(
                &[star_filter(MatchType::All, vec![cond(field, op, value)])],
                &facts
            ),
            expected,
            "field {field:?} op {op:?} value {value:?}"
        );
    }
}

#[test]
fn numeric_attachment_and_regex_ops() {
    let facts = sample_facts();
    let cases = [
        (ConditionField::Size, ConditionOp::GreaterThan, "1000", true),
        (ConditionField::Size, ConditionOp::LessThan, "1000", false),
        (
            ConditionField::Date,
            ConditionOp::LessThan,
            "2024-01-01",
            true,
        ),
        (
            ConditionField::Date,
            ConditionOp::GreaterThan,
            "2024-01-01",
            false,
        ),
        (ConditionField::HasAttachment, ConditionOp::Is, "true", true),
        (
            ConditionField::HasAttachment,
            ConditionOp::IsNot,
            "true",
            false,
        ),
        (
            ConditionField::From,
            ConditionOp::MatchesRegex,
            r"news@.*\.com",
            true,
        ),
        (
            ConditionField::Subject,
            ConditionOp::MatchesRegex,
            r"^daily",
            false,
        ),
    ];
    for (field, op, value, expected) in cases {
        assert_eq!(
            matches(
                &[star_filter(MatchType::All, vec![cond(field, op, value)])],
                &facts
            ),
            expected,
            "field {field:?} op {op:?} value {value:?}"
        );
    }
}

/// Regression (finding #7): a regex pattern must be compiled verbatim, never
/// lowercased — lowercasing corrupts uppercase escape classes (`\D`→`\d`), which
/// would silently invert the match set of a possibly-destructive rule.
#[test]
fn regex_uppercase_escape_class_is_not_corrupted() {
    let non_digit = FilterMessageFacts {
        subject: "abc".to_owned(),
        ..Default::default()
    };
    let all_digits = FilterMessageFacts {
        subject: "12345".to_owned(),
        ..Default::default()
    };
    let re = |pattern: &str| {
        [star_filter(
            MatchType::All,
            vec![cond(
                ConditionField::Subject,
                ConditionOp::MatchesRegex,
                pattern,
            )],
        )]
    };
    // `\D+` matches non-digits (the correct behaviour), not digits.
    assert!(
        matches(&re(r"\D+"), &non_digit),
        "\\D+ must match non-digits"
    );
    assert!(
        !matches(&re(r"\D+"), &all_digits),
        "\\D+ must NOT match an all-digit string"
    );
    // Case-insensitivity still works for literal patterns (compiled with the
    // case-insensitive flag rather than by mangling the pattern text).
    assert!(
        matches(&re("ABC"), &non_digit),
        "an uppercase literal still matches case-insensitively"
    );
}

#[test]
fn all_versus_any_combines_conditions() {
    let facts = sample_facts();
    let mixed = vec![
        cond(ConditionField::Subject, ConditionOp::Contains, "weekly"),
        cond(ConditionField::Subject, ConditionOp::Contains, "absent"),
    ];
    assert!(!matches(
        &[star_filter(MatchType::All, mixed.clone())],
        &facts
    ));
    assert!(matches(&[star_filter(MatchType::Any, mixed)], &facts));
}

#[test]
fn stop_on_match_halts_later_filters() {
    let facts = sample_facts();
    let mut first = star_filter(
        MatchType::All,
        vec![cond(
            ConditionField::Subject,
            ConditionOp::Contains,
            "weekly",
        )],
    );
    first.stop_on_match = true;
    let mut second = star_filter(
        MatchType::All,
        vec![cond(
            ConditionField::Subject,
            ConditionOp::Contains,
            "weekly",
        )],
    );
    second.actions = vec![FilterAction {
        kind: ActionKind::MarkRead,
        param: String::new(),
    }];
    let actions = FilterProgram::compile(&[first, second]).evaluate(&facts);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, ActionKind::Star);
}

#[test]
fn a_stop_action_also_halts_later_filters() {
    let facts = sample_facts();
    let mut first = star_filter(
        MatchType::All,
        vec![cond(
            ConditionField::Subject,
            ConditionOp::Contains,
            "weekly",
        )],
    );
    first.actions.push(FilterAction {
        kind: ActionKind::Stop,
        param: String::new(),
    });
    let second = star_filter(
        MatchType::All,
        vec![cond(
            ConditionField::Subject,
            ConditionOp::Contains,
            "weekly",
        )],
    );
    // First filter emits [Star, Stop]; the second must not contribute.
    let actions = FilterProgram::compile(&[first, second]).evaluate(&facts);
    assert_eq!(actions.len(), 2);
}

#[test]
fn a_conditionless_filter_never_matches() {
    assert!(!matches(
        &[star_filter(MatchType::All, Vec::new())],
        &sample_facts()
    ));
}

// ---------------------------------------------------------------------------
// Repository CRUD, reorder, scope
// ---------------------------------------------------------------------------

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

fn input(account: Option<AccountId>, name: &str) -> FilterInput {
    FilterInput {
        account,
        name: name.to_owned(),
        enabled: true,
        match_type: MatchType::All,
        stop_on_match: false,
        conditions: vec![cond(
            ConditionField::Subject,
            ConditionOp::Contains,
            "invoice",
        )],
        actions: vec![FilterAction {
            kind: ActionKind::Star,
            param: String::new(),
        }],
    }
}

#[tokio::test]
async fn create_read_update_delete_round_trip() {
    let (store, account, _) = setup().await;
    let created = store
        .filters()
        .create(&input(Some(account), "Invoices"))
        .await
        .unwrap();
    assert_eq!(created.conditions.len(), 1);
    assert_eq!(created.actions.len(), 1);

    let fetched = store.filters().get(created.id).await.unwrap().unwrap();
    assert_eq!(fetched, created);

    let mut edit = input(Some(account), "Renamed");
    edit.conditions.push(cond(
        ConditionField::From,
        ConditionOp::Contains,
        "billing@",
    ));
    edit.match_type = MatchType::Any;
    let updated = store
        .filters()
        .update(created.id, &edit)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.match_type, MatchType::Any);
    assert_eq!(updated.conditions.len(), 2);

    assert!(store.filters().delete(created.id).await.unwrap());
    assert!(store.filters().get(created.id).await.unwrap().is_none());
}

#[tokio::test]
async fn reorder_and_enable_toggle() {
    let (store, account, _) = setup().await;
    let a = store
        .filters()
        .create(&input(Some(account), "A"))
        .await
        .unwrap();
    let b = store
        .filters()
        .create(&input(Some(account), "B"))
        .await
        .unwrap();
    let c = store
        .filters()
        .create(&input(Some(account), "C"))
        .await
        .unwrap();

    // Default order is creation order.
    let names: Vec<_> = store
        .filters()
        .list(Some(account))
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["A", "B", "C"]);

    store
        .filters()
        .reorder(Some(account), &[c.id, a.id, b.id])
        .await
        .unwrap();
    let names: Vec<_> = store
        .filters()
        .list(Some(account))
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["C", "A", "B"]);

    store.filters().set_enabled(b.id, false).await.unwrap();
    let active: Vec<_> = store
        .filters()
        .active_for_account(account)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(active, ["C", "A"]);
}

/// Regression (finding #11): a partial reorder renumbers the *whole* scope to a
/// dense, unique sequence, so no stale/duplicate sort_order can leave the
/// (destructive) rule order ambiguous.
#[tokio::test]
async fn partial_reorder_renumbers_all_rules_densely() {
    let (store, account, _) = setup().await;
    let a = store
        .filters()
        .create(&input(Some(account), "A"))
        .await
        .unwrap();
    let b = store
        .filters()
        .create(&input(Some(account), "B"))
        .await
        .unwrap();
    let _c = store
        .filters()
        .create(&input(Some(account), "C"))
        .await
        .unwrap();
    let d = store
        .filters()
        .create(&input(Some(account), "D"))
        .await
        .unwrap();

    // Mention only two ids; the rest must keep their prior relative order after.
    store
        .filters()
        .reorder(Some(account), &[d.id, b.id])
        .await
        .unwrap();

    let filters = store.filters().list(Some(account)).await.unwrap();
    let names: Vec<_> = filters.iter().map(|f| f.name.clone()).collect();
    assert_eq!(
        names,
        ["D", "B", "A", "C"],
        "requested ids lead, rest follow"
    );
    // Every sort_order is unique and contiguous 0..n.
    let orders: Vec<i64> = filters.iter().map(|f| f.sort_order).collect();
    assert_eq!(orders, [0, 1, 2, 3], "dense, unique sort orders");
    // Unrelated ids in the reorder list are ignored (no panic, no effect here).
    let _ = a;
}

#[tokio::test]
async fn account_scoped_rules_precede_global_rules() {
    let (store, account, _) = setup().await;
    store
        .filters()
        .create(&input(Some(account), "Account rule"))
        .await
        .unwrap();
    store
        .filters()
        .create(&input(None, "Global rule"))
        .await
        .unwrap();
    let active: Vec<_> = store
        .filters()
        .active_for_account(account)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(active, ["Account rule", "Global rule"]);
    // The account-scoped list excludes the global rule.
    assert_eq!(store.filters().list(Some(account)).await.unwrap().len(), 1);
    assert_eq!(store.filters().list(None).await.unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_an_account_cascades_its_filters() {
    let (store, account, _) = setup().await;
    let filter = store
        .filters()
        .create(&input(Some(account), "Doomed"))
        .await
        .unwrap();
    store.accounts().delete(account).await.unwrap();
    assert!(store.filters().get(filter.id).await.unwrap().is_none());
}

#[test]
fn validation_rejects_bad_filters() {
    let mut blank = input(None, "  ");
    assert!(validate_filter_input(&blank).is_err());
    blank.name = "ok".to_owned();
    blank.conditions.clear();
    assert!(validate_filter_input(&blank).is_err());

    let mut bad_regex = input(None, "ok");
    bad_regex.conditions = vec![cond(
        ConditionField::Subject,
        ConditionOp::MatchesRegex,
        "(",
    )];
    assert!(validate_filter_input(&bad_regex).is_err());

    assert!(validate_filter_input(&input(None, "ok")).is_ok());
}

// ---------------------------------------------------------------------------
// Facts loader
// ---------------------------------------------------------------------------

#[tokio::test]
async fn facts_loader_reflects_stored_message() {
    let (store, account, mailbox) = setup().await;
    let mut message = sample_message(mailbox, "Quarterly Report", Some(1_700_000_000));
    message.cc = vec![axomail_core::Address::from_email(
        axomail_core::EmailAddress::parse("cc@example.org").unwrap(),
    )];
    let id = store.messages().insert(&message).await.unwrap().id;

    // Attach a tag and confirm the facts pick it up.
    let tag = store
        .tags()
        .ensure_by_name(account, "Reports")
        .await
        .unwrap();
    store.tags().add_message_tag(id, tag.id).await.unwrap();

    let facts = store
        .filters()
        .facts_for_message(id)
        .await
        .unwrap()
        .unwrap();
    assert!(facts.subject.contains("quarterly report"));
    assert!(facts.from.iter().any(|f| f.contains("ada@example.org")));
    assert!(facts.cc.iter().any(|c| c.contains("cc@example.org")));
    assert!(facts.tags.iter().any(|t| t == "reports"));
    assert_eq!(facts.size, Some(2048));
    assert_eq!(facts.date, Some(1_700_000_000));
    assert!(!facts.has_attachment);

    assert!(
        store
            .filters()
            .facts_for_message(axomail_core::MessageId::new(9999))
            .await
            .unwrap()
            .is_none()
    );
}

// ---------------------------------------------------------------------------
// Run-on-arrival state
// ---------------------------------------------------------------------------

async fn insert_at(store: &Store, mailbox: MailboxId, subject: &str) -> i64 {
    let mut message = NewMessage {
        flags: MessageFlags::default(),
        date: Some(UtcTimestamp::from_unix_seconds(1)),
        ..sample_message(mailbox, subject, Some(1))
    };
    message.subject = Some(subject.to_owned());
    store.messages().insert(&message).await.unwrap().id.value()
}

#[tokio::test]
async fn arrival_state_baseline_toggle_and_advance() {
    let (store, account, mailbox) = setup().await;
    // No row yet: enabled defaults to true, and there is no state.
    assert!(store.filters().arrival_enabled(account).await.unwrap());
    assert!(
        store
            .filters()
            .arrival_state(account)
            .await
            .unwrap()
            .is_none()
    );

    let first = insert_at(&store, mailbox, "one").await;
    let second = insert_at(&store, mailbox, "two").await;
    assert_eq!(
        store
            .filters()
            .max_message_id_in_mailbox(mailbox)
            .await
            .unwrap(),
        second
    );

    // Baseline to the current frontier: nothing before it is a candidate.
    store
        .filters()
        .baseline_arrival(account, second)
        .await
        .unwrap();
    let candidates = store
        .filters()
        .messages_in_mailbox_after(mailbox, second)
        .await
        .unwrap();
    assert!(candidates.is_empty());

    // A newly-arrived message is a candidate; advancing the mark clears it.
    let third = insert_at(&store, mailbox, "three").await;
    let candidates = store
        .filters()
        .messages_in_mailbox_after(mailbox, second)
        .await
        .unwrap();
    assert_eq!(
        candidates.iter().map(|id| id.value()).collect::<Vec<_>>(),
        [third]
    );
    store
        .filters()
        .advance_arrival(account, third)
        .await
        .unwrap();
    assert!(
        store
            .filters()
            .messages_in_mailbox_after(mailbox, third)
            .await
            .unwrap()
            .is_empty()
    );

    // Toggling off is reflected; the mark survives.
    store
        .filters()
        .set_arrival_enabled(account, false, first)
        .await
        .unwrap();
    let state = store
        .filters()
        .arrival_state(account)
        .await
        .unwrap()
        .unwrap();
    assert!(!state.enabled);
    assert_eq!(state.last_message_id, third);
}
