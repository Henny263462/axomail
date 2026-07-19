//! Tag commands: per-account tag CRUD, the per-message picker options and
//! tag/untag mutation (which also enqueues the server keyword write), and the
//! tag-filter message listing.

use std::collections::HashSet;

use axomail_core::{AccountId, Keyword, MailError, MessageId, RemoteMessageId};
use axomail_storage::{
    KeywordOpPayload, NewPendingOp, NewTag, PendingOpKind, Store, TagColor, TagId, TagUpdate,
};

use crate::commands::map_command_error;
use crate::dto::{MessageListPageDto, MessageTagOptionDto, MessageTagOptionsDto, TagDto};
use crate::state::AppState;

/// Cap on the tag-filter listing (a snapshot view, not paginated).
const TAG_FILTER_LIMIT: u32 = 500;

/// Lists an account's tags with their usage counts, for the tag manager.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_tags_impl(store: &Store, account_id: i64) -> Result<Vec<TagDto>, MailError> {
    store
        .tags()
        .list_with_usage(AccountId::new(account_id))
        .await?
        .iter()
        .map(|(tag, usage)| TagDto::from_tag(tag, *usage))
        .collect()
}

/// Lists every tag across all accounts (with usage), for the sidebar filter.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_all_tags_impl(store: &Store) -> Result<Vec<TagDto>, MailError> {
    store
        .tags()
        .list_all_with_usage()
        .await?
        .iter()
        .map(|(tag, usage)| TagDto::from_tag(tag, *usage))
        .collect()
}

/// Creates a tag for an account: validates its name and color, derives a
/// unique IMAP keyword from the name, and appends it to the account's list.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown account, a blank name,
/// or an invalid color, and [`MailError::Database`] on any query failure.
pub async fn create_tag_impl(
    store: &Store,
    account_id: i64,
    name: String,
    color: Option<String>,
) -> Result<TagDto, MailError> {
    let account = AccountId::new(account_id);
    if store.accounts().get(account).await?.is_none() {
        return Err(MailError::Configuration {
            message: "cannot add a tag to an account that does not exist".to_owned(),
        });
    }
    let display_name = non_blank(&name, "tag name")?;
    let color = parse_optional_color(color.as_deref())?;
    let keyword = unique_keyword(store, account, &display_name).await?;
    let sort_order = i64::try_from(store.tags().list(account).await?.len()).unwrap_or(0);
    let tag = store
        .tags()
        .create(&NewTag {
            account,
            keyword,
            display_name,
            color,
            sort_order,
        })
        .await?;
    TagDto::from_tag(&tag, 0)
}

/// Updates a tag's display name and color (its keyword is immutable).
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for a blank name, invalid color, or an
/// unknown tag, and [`MailError::Database`] on any query failure.
pub async fn update_tag_impl(
    store: &Store,
    tag_id: i64,
    display_name: String,
    color: Option<String>,
) -> Result<TagDto, MailError> {
    let id = TagId::new(tag_id);
    let update = TagUpdate {
        display_name: non_blank(&display_name, "tag name")?,
        color: parse_optional_color(color.as_deref())?,
    };
    let tag = store
        .tags()
        .update(id, &update)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot update a tag that does not exist".to_owned(),
        })?;
    let usage = store.tags().usage_count(id).await?;
    TagDto::from_tag(&tag, usage)
}

/// Deletes a tag, cascading to its message associations. Deleting an absent
/// tag is a silent no-op.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure.
pub async fn delete_tag_impl(store: &Store, tag_id: i64) -> Result<(), MailError> {
    store.tags().delete(TagId::new(tag_id)).await?;
    Ok(())
}

/// Builds the tag picker's payload for one message: its account plus every
/// account tag with whether the message currently carries it.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown message and
/// [`MailError::Database`] on any query failure.
pub async fn message_tag_options_impl(
    store: &Store,
    message_id: i64,
) -> Result<MessageTagOptionsDto, MailError> {
    let id = MessageId::new(message_id);
    let account = message_account(store, id).await?;
    let tags = store.tags().list(account).await?;
    let applied: HashSet<i64> = store
        .tags()
        .tags_for_message(id)
        .await?
        .iter()
        .map(|tag| tag.id.value())
        .collect();
    let options = tags
        .iter()
        .map(|tag| {
            Ok(MessageTagOptionDto {
                id: crate::dto::checked_id(tag.id.value())?,
                display_name: tag.display_name.clone(),
                color: tag.color.as_ref().map(|c| c.as_str().to_owned()),
                applied: applied.contains(&tag.id.value()),
            })
        })
        .collect::<Result<Vec<_>, MailError>>()?;
    Ok(MessageTagOptionsDto {
        message_id: crate::dto::checked_id(id.value())?,
        account_id: crate::dto::checked_id(account.value())?,
        options,
    })
}

/// Applies or removes a tag on a message: updates the local association and,
/// for an IMAP message, enqueues an additive keyword write for the server.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown message or tag, or a tag
/// that belongs to a different account than the message, and
/// [`MailError::Database`] on any query failure.
pub async fn set_message_tag_impl(
    store: &Store,
    message_id: i64,
    tag_id: i64,
    tagged: bool,
) -> Result<(), MailError> {
    let msg_id = MessageId::new(message_id);
    let tag_id = TagId::new(tag_id);
    let envelope = store
        .messages()
        .get(msg_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot tag a message that does not exist".to_owned(),
        })?;
    let mailbox = store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .ok_or_else(|| MailError::Database {
            message: "set_message_tag: message mailbox is missing".to_owned(),
        })?;
    let tag = store
        .tags()
        .get(tag_id)
        .await?
        .ok_or_else(|| MailError::Configuration {
            message: "cannot apply a tag that does not exist".to_owned(),
        })?;
    if tag.account != mailbox.account {
        return Err(MailError::Configuration {
            message: "the tag belongs to a different account than the message".to_owned(),
        });
    }

    if tagged {
        store.tags().add_message_tag(msg_id, tag_id).await?;
    } else {
        store.tags().remove_message_tag(msg_id, tag_id).await?;
    }

    // Keywords are an IMAP concept: only enqueue a server write for an IMAP
    // message. Local-only and POP3 messages keep the tag locally.
    if matches!(envelope.remote_id, Some(RemoteMessageId::Imap { .. })) {
        let kind = if tagged {
            PendingOpKind::AddKeyword
        } else {
            PendingOpKind::RemoveKeyword
        };
        let payload = KeywordOpPayload {
            keyword: tag.keyword.clone(),
        }
        .to_json()?;
        store
            .pending_ops()
            .enqueue(&NewPendingOp {
                account_id: mailbox.account,
                message_id: msg_id,
                kind,
                payload_json: payload,
            })
            .await?;
    }
    Ok(())
}

/// Lists the messages carrying a tag (newest first, capped), for the sidebar
/// tag-filter view. Rows carry their tag chips.
///
/// # Errors
///
/// Returns [`MailError::Database`] on any query failure and a safe-integer
/// error if a stored id cannot cross to the frontend.
pub async fn list_messages_for_tag_impl(
    store: &Store,
    tag_id: i64,
) -> Result<MessageListPageDto, MailError> {
    let envelopes = store
        .tags()
        .messages_for_tag(TagId::new(tag_id), TAG_FILTER_LIMIT)
        .await?;
    let ids: Vec<MessageId> = envelopes.iter().map(|envelope| envelope.id).collect();
    let tags = store.tags().tags_for_messages(&ids).await?;
    MessageListPageDto::from_envelopes_with_tags(&envelopes, &tags)
}

/// Resolves the owning account of a message from its mailbox.
async fn message_account(store: &Store, message: MessageId) -> Result<AccountId, MailError> {
    let envelope =
        store
            .messages()
            .get(message)
            .await?
            .ok_or_else(|| MailError::Configuration {
                message: "message does not exist".to_owned(),
            })?;
    let mailbox = store
        .mailboxes()
        .get(envelope.mailbox)
        .await?
        .ok_or_else(|| MailError::Database {
            message: "message mailbox is missing".to_owned(),
        })?;
    Ok(mailbox.account)
}

/// Trims and rejects a blank required field.
fn non_blank(value: &str, field: &str) -> Result<String, MailError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MailError::Configuration {
            message: format!("{field} must not be empty"),
        });
    }
    Ok(trimmed.to_owned())
}

/// Validates an optional `#rrggbb` color, treating blank as absent.
fn parse_optional_color(color: Option<&str>) -> Result<Option<TagColor>, MailError> {
    match color.map(str::trim) {
        Some(value) if !value.is_empty() => Ok(Some(TagColor::parse(value)?)),
        _ => Ok(None),
    }
}

/// Derives a unique-per-account IMAP keyword from a tag name: keeps the name's
/// valid atom characters, falls back to `label` when nothing valid remains,
/// then appends a numeric suffix until the keyword is free in the account.
async fn unique_keyword(
    store: &Store,
    account: AccountId,
    name: &str,
) -> Result<Keyword, MailError> {
    let base = match Keyword::parse(&keyword_atom(name)) {
        Ok(keyword) => keyword.as_str().to_owned(),
        Err(_) => "label".to_owned(),
    };
    let mut candidate = base.clone();
    let mut suffix = 1u32;
    loop {
        let keyword = Keyword::parse(&candidate)?;
        if store
            .tags()
            .find_by_keyword(account, &keyword)
            .await?
            .is_none()
        {
            return Ok(keyword);
        }
        suffix += 1;
        candidate = format!("{base}{suffix}");
    }
}

/// Filters a name down to the RFC 3501 atom charset (dropping spaces and
/// atom-special characters), the basis for a derived keyword.
fn keyword_atom(name: &str) -> String {
    name.chars()
        .filter(|ch| {
            ch.is_ascii()
                && !ch.is_ascii_control()
                && !matches!(ch, '(' | ')' | '{' | ' ' | '%' | '*' | '"' | '\\' | ']')
        })
        .collect()
}

/// Lists an account's tags with usage counts.
#[tauri::command]
pub async fn list_tags(
    state: tauri::State<'_, AppState>,
    account_id: i64,
) -> Result<Vec<TagDto>, String> {
    list_tags_impl(state.store(), account_id)
        .await
        .map_err(map_command_error)
}

/// Lists every tag across all accounts.
#[tauri::command]
pub async fn list_all_tags(state: tauri::State<'_, AppState>) -> Result<Vec<TagDto>, String> {
    list_all_tags_impl(state.store())
        .await
        .map_err(map_command_error)
}

/// Creates a tag for an account.
#[tauri::command]
pub async fn create_tag(
    state: tauri::State<'_, AppState>,
    account_id: i64,
    name: String,
    color: Option<String>,
) -> Result<TagDto, String> {
    create_tag_impl(state.store(), account_id, name, color)
        .await
        .map_err(map_command_error)
}

/// Updates a tag's display name and color.
#[tauri::command]
pub async fn update_tag(
    state: tauri::State<'_, AppState>,
    tag_id: i64,
    display_name: String,
    color: Option<String>,
) -> Result<TagDto, String> {
    update_tag_impl(state.store(), tag_id, display_name, color)
        .await
        .map_err(map_command_error)
}

/// Deletes a tag.
#[tauri::command]
pub async fn delete_tag(state: tauri::State<'_, AppState>, tag_id: i64) -> Result<(), String> {
    delete_tag_impl(state.store(), tag_id)
        .await
        .map_err(map_command_error)
}

/// Returns the tag picker options for a message.
#[tauri::command]
pub async fn message_tag_options(
    state: tauri::State<'_, AppState>,
    message_id: i64,
) -> Result<MessageTagOptionsDto, String> {
    message_tag_options_impl(state.store(), message_id)
        .await
        .map_err(map_command_error)
}

/// Applies or removes a tag on a message.
#[tauri::command]
pub async fn set_message_tag(
    state: tauri::State<'_, AppState>,
    message_id: i64,
    tag_id: i64,
    tagged: bool,
) -> Result<(), String> {
    set_message_tag_impl(state.store(), message_id, tag_id, tagged)
        .await
        .map_err(map_command_error)
}

/// Lists the messages carrying a tag.
#[tauri::command]
pub async fn list_messages_for_tag(
    state: tauri::State<'_, AppState>,
    tag_id: i64,
) -> Result<MessageListPageDto, String> {
    list_messages_for_tag_impl(state.store(), tag_id)
        .await
        .map_err(map_command_error)
}
