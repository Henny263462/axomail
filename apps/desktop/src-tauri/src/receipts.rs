//! Read-receipt (MDN, RFC 8098) honour policy and its persisted state.
//!
//! This module owns the small, self-contained pieces of the receipt feature:
//! the honour policy + its two conditions stored in the `app_settings` KV table,
//! the per-message "receipt already sent" marker (the local half of the
//! `$MDNSent` keyword, so a receipt is never sent twice), the pure
//! [`decide`] rule that maps a policy and a message's properties to an action,
//! and [`receipt_request_for`], which builds the reading-pane request DTO.
//!
//! Persisting these in the KV store keeps the track migration-free (its ledger
//! row assigns it no migration); nothing here touches the schema.

use std::collections::HashSet;

use axomail_core::{Account, MailError, MailboxId};
use axomail_parser::ParsedMessage;
use axomail_storage::Store;

use crate::dto::{ReceiptPolicy, ReceiptRequestDto, ReceiptSettingsDto};

/// Base receipt policy (`"never"` / `"ask"` / `"always"`).
const KEY_POLICY: &str = "receipt_policy";
/// Whether a request is honoured only when the message is addressed to me.
const KEY_ONLY_WHEN_ADDRESSED: &str = "receipt_only_when_addressed";
/// Whether a request is never honoured for mailing-list / bulk mail.
const KEY_NEVER_FOR_LISTS: &str = "receipt_never_for_mailing_lists";
/// Prefix of the per-message "a receipt was already sent" marker key.
const KEY_MDN_SENT_PREFIX: &str = "mdn_sent.";

/// The action to take for a message that requested a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptAction {
    /// Do not send a receipt (policy `Never`, or a condition excluded it).
    Ignore,
    /// Surface the request and wait for the user to confirm (`Ask`).
    Ask,
    /// Send a receipt automatically (`Always`).
    Send,
}

/// Decides how to honour a receipt request from the policy and the message's
/// properties. Pure: the same inputs always yield the same action.
///
/// The two conditions are applied first and can only *suppress*: a mailing-list
/// message (when `never_for_mailing_lists`) or a message not addressed to me
/// (when `only_when_addressed`) is always ignored, regardless of policy.
/// Otherwise the base policy decides.
#[must_use]
pub fn decide(
    settings: ReceiptSettingsDto,
    addressed_to_me: bool,
    is_mailing_list: bool,
) -> ReceiptAction {
    if settings.never_for_mailing_lists && is_mailing_list {
        return ReceiptAction::Ignore;
    }
    if settings.only_when_addressed && !addressed_to_me {
        return ReceiptAction::Ignore;
    }
    match settings.policy {
        ReceiptPolicy::Never => ReceiptAction::Ignore,
        ReceiptPolicy::Ask => ReceiptAction::Ask,
        ReceiptPolicy::Always => ReceiptAction::Send,
    }
}

/// Reads the receipt honour settings, falling back to the privacy-preserving
/// defaults for absent keys.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn settings(store: &Store) -> Result<ReceiptSettingsDto, MailError> {
    let defaults = ReceiptSettingsDto::default();
    let policy = store
        .settings()
        .get(KEY_POLICY)
        .await?
        .map_or(defaults.policy, |value| ReceiptPolicy::from_key(&value));
    let only_when_addressed =
        read_bool(store, KEY_ONLY_WHEN_ADDRESSED, defaults.only_when_addressed).await?;
    let never_for_mailing_lists =
        read_bool(store, KEY_NEVER_FOR_LISTS, defaults.never_for_mailing_lists).await?;
    Ok(ReceiptSettingsDto {
        policy,
        only_when_addressed,
        never_for_mailing_lists,
    })
}

/// Persists the receipt honour settings.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn set_settings(store: &Store, value: ReceiptSettingsDto) -> Result<(), MailError> {
    store
        .settings()
        .set(KEY_POLICY, value.policy.as_key())
        .await?;
    store
        .settings()
        .set(KEY_ONLY_WHEN_ADDRESSED, bool_str(value.only_when_addressed))
        .await?;
    store
        .settings()
        .set(KEY_NEVER_FOR_LISTS, bool_str(value.never_for_mailing_lists))
        .await
}

/// Whether a receipt has already been sent for this message (the local mirror
/// of the `$MDNSent` keyword).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn mdn_already_sent(store: &Store, message_id: i64) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(&mdn_sent_key(message_id))
        .await?
        .is_some_and(|value| value == "true"))
}

/// Records that a receipt was sent for this message, so it is never sent again.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn mark_mdn_sent(store: &Store, message_id: i64) -> Result<(), MailError> {
    store
        .settings()
        .set(&mdn_sent_key(message_id), "true")
        .await
}

/// Atomically claims the sending of a receipt for this message.
///
/// Returns `true` when this caller won the claim and must proceed to send the
/// receipt exactly once; `false` when a receipt was already sent (or another
/// send is in flight). This closes the double-send race: the "already sent?"
/// check and the marker write are a single atomic insert, so two concurrent
/// `send_mdn` calls cannot both send. A caller that wins but then fails to send
/// must [`release_mdn_claim`] so a later retry can send.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn claim_mdn_send(store: &Store, message_id: i64) -> Result<bool, MailError> {
    store
        .settings()
        .set_if_absent(&mdn_sent_key(message_id), "true")
        .await
}

/// Releases a claim taken by [`claim_mdn_send`] when the send did not succeed,
/// so a later attempt may send the receipt.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn release_mdn_claim(store: &Store, message_id: i64) -> Result<(), MailError> {
    store.settings().delete(&mdn_sent_key(message_id)).await
}

/// Builds the reading-pane read-receipt request for a message, or `None` when
/// none should be surfaced (no request header, already sent, or the policy
/// decided to ignore it).
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn receipt_request_for(
    store: &Store,
    message_id: i64,
    mailbox_id: MailboxId,
    parsed: &ParsedMessage,
) -> Result<Option<ReceiptRequestDto>, MailError> {
    let Some(requester) = parsed.disposition_notification_to.first() else {
        return Ok(None);
    };
    if mdn_already_sent(store, message_id).await? {
        return Ok(None);
    }
    let Some(account) = account_of_mailbox(store, mailbox_id).await? else {
        return Ok(None);
    };
    let mine = self_emails(store, &account).await?;
    let addressed_to_me = parsed
        .to
        .iter()
        .chain(&parsed.cc)
        .any(|address| mine.contains(&address.email.as_str().to_ascii_lowercase()));
    let action = decide(settings(store).await?, addressed_to_me, parsed.mailing_list);
    Ok(match action {
        ReceiptAction::Ignore => None,
        ReceiptAction::Ask => Some(ReceiptRequestDto {
            to: requester.email.as_str().to_owned(),
            action: "ask",
        }),
        ReceiptAction::Send => Some(ReceiptRequestDto {
            to: requester.email.as_str().to_owned(),
            action: "send",
        }),
    })
}

/// The set of the account's own addresses (account address plus every
/// identity), lowercased for case-insensitive comparison.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn self_emails(store: &Store, account: &Account) -> Result<HashSet<String>, MailError> {
    let mut set = HashSet::new();
    set.insert(account.email.as_str().to_ascii_lowercase());
    for identity in store.identities().list(account.id).await? {
        set.insert(identity.fields.email.to_ascii_lowercase());
    }
    Ok(set)
}

/// Loads the account that owns a mailbox, if both still exist.
async fn account_of_mailbox(
    store: &Store,
    mailbox_id: MailboxId,
) -> Result<Option<Account>, MailError> {
    let Some(mailbox) = store.mailboxes().get(mailbox_id).await? else {
        return Ok(None);
    };
    store.accounts().get(mailbox.account).await
}

/// Reads a boolean setting, using `default` for an absent key.
async fn read_bool(store: &Store, key: &str, default: bool) -> Result<bool, MailError> {
    Ok(store
        .settings()
        .get(key)
        .await?
        .map_or(default, |value| value == "true"))
}

/// Canonical string form of a boolean setting.
fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Builds the per-message MDN-sent marker key.
fn mdn_sent_key(message_id: i64) -> String {
    format!("{KEY_MDN_SENT_PREFIX}{message_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(
        policy: ReceiptPolicy,
        only_when_addressed: bool,
        never_for_mailing_lists: bool,
    ) -> ReceiptSettingsDto {
        ReceiptSettingsDto {
            policy,
            only_when_addressed,
            never_for_mailing_lists,
        }
    }

    #[test]
    fn never_policy_always_ignores() {
        let s = settings_with(ReceiptPolicy::Never, false, false);
        assert_eq!(decide(s, true, false), ReceiptAction::Ignore);
        assert_eq!(decide(s, false, true), ReceiptAction::Ignore);
    }

    #[test]
    fn ask_policy_asks_when_no_condition_excludes() {
        let s = settings_with(ReceiptPolicy::Ask, true, true);
        assert_eq!(decide(s, true, false), ReceiptAction::Ask);
    }

    #[test]
    fn always_policy_sends_when_no_condition_excludes() {
        let s = settings_with(ReceiptPolicy::Always, true, true);
        assert_eq!(decide(s, true, false), ReceiptAction::Send);
    }

    #[test]
    fn mailing_list_condition_suppresses_even_for_always() {
        let s = settings_with(ReceiptPolicy::Always, false, true);
        assert_eq!(decide(s, true, true), ReceiptAction::Ignore);
        // With the condition off, a list message is honoured again.
        let s = settings_with(ReceiptPolicy::Always, false, false);
        assert_eq!(decide(s, true, true), ReceiptAction::Send);
    }

    #[test]
    fn addressed_to_me_condition_suppresses_when_not_addressed() {
        let s = settings_with(ReceiptPolicy::Ask, true, false);
        assert_eq!(decide(s, false, false), ReceiptAction::Ignore);
        assert_eq!(decide(s, true, false), ReceiptAction::Ask);
    }

    #[test]
    fn mailing_list_takes_precedence_over_addressing() {
        // A list message addressed to me is still suppressed by the list rule.
        let s = settings_with(ReceiptPolicy::Always, true, true);
        assert_eq!(decide(s, true, true), ReceiptAction::Ignore);
    }
}
