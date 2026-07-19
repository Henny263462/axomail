//! Seeds an account's Thunderbird-default-like starter tags on creation.
//!
//! Kept separate from the account-creation command so its static table and
//! seed loop don't crowd `accounts.rs`. The keywords are Thunderbird's own
//! `$labelN` custom flags, so a message tagged here lights up the same label
//! in a Thunderbird client synced to the same IMAP account.

use axomail_core::{AccountId, Keyword, MailError};
use axomail_storage::{NewTag, Store, TagColor};

/// Starter tags as `(keyword, display name, color)`, in display order.
const DEFAULT_TAGS: [(&str, &str, &str); 5] = [
    ("$label1", "Important", "#e04b4b"),
    ("$label2", "Work", "#e08a3c"),
    ("$label3", "Personal", "#3caf5a"),
    ("$label4", "To Do", "#3c78d8"),
    ("$label5", "Later", "#8a56d6"),
];

/// Creates the starter tag set for a freshly inserted account, so the user has
/// a usable colored-label set before the first sync. Keyword and color are
/// static and known-valid, so parsing them never fails at runtime.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub(crate) async fn seed_default_tags(
    store: &Store,
    account_id: AccountId,
) -> Result<(), MailError> {
    for (index, (keyword, display_name, color)) in DEFAULT_TAGS.iter().enumerate() {
        store
            .tags()
            .create(&NewTag {
                account: account_id,
                keyword: Keyword::parse(keyword)?,
                display_name: (*display_name).to_owned(),
                color: Some(TagColor::parse(color)?),
                sort_order: i64::try_from(index).unwrap_or(0),
            })
            .await?;
    }
    Ok(())
}
