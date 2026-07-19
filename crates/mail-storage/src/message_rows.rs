//! Row mapping for the message repository: raw `messages` and
//! `message_recipients` rows and their conversion to `axomail_core`
//! domain types. Kept separate from the query logic in `messages` to
//! keep both files small.

use std::collections::HashMap;

use axomail_core::{
    Address, EmailAddress, MailError, MailboxId, MessageEnvelope, MessageFlags, MessageId,
    RemoteMessageId, UtcTimestamp,
};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;
use crate::pagination::NULL_DATE_SORT;
use crate::threading_json::decode_threading;

/// Columns selected when reading a message, in [`MessageRow`] order.
pub(crate) const MESSAGE_COLUMNS: &str = "id, mailbox_id, \
    remote_kind, remote_uid, remote_uid_validity, remote_uidl, \
    subject, date_unix, \
    flag_seen, flag_answered, flag_flagged, flag_draft, flag_deleted, \
    size_bytes, preview, \
    message_id, in_reply_to, references_json";

/// [`MESSAGE_COLUMNS`] qualified with the `m` table alias, for queries
/// that join `messages` against other tables (e.g. the unified inbox).
/// Output column names stay unqualified, so [`MessageRow`] maps them by
/// the same names as [`MESSAGE_COLUMNS`].
pub(crate) const MESSAGE_COLUMNS_M: &str = "m.id, m.mailbox_id, \
    m.remote_kind, m.remote_uid, m.remote_uid_validity, m.remote_uidl, \
    m.subject, m.date_unix, \
    m.flag_seen, m.flag_answered, m.flag_flagged, m.flag_draft, m.flag_deleted, \
    m.size_bytes, m.preview, \
    m.message_id, m.in_reply_to, m.references_json";

/// Raw `messages` row, mapped by column name.
#[derive(sqlx::FromRow)]
pub(crate) struct MessageRow {
    /// Row identifier.
    pub(crate) id: i64,
    mailbox_id: i64,
    remote_kind: Option<String>,
    remote_uid: Option<i64>,
    remote_uid_validity: Option<i64>,
    remote_uidl: Option<String>,
    subject: Option<String>,
    date_unix: Option<i64>,
    flag_seen: bool,
    flag_answered: bool,
    flag_flagged: bool,
    flag_draft: bool,
    flag_deleted: bool,
    size_bytes: Option<i64>,
    preview: Option<String>,
    message_id: Option<String>,
    in_reply_to: Option<String>,
    references_json: Option<String>,
}

impl MessageRow {
    /// Coalesced sort date used for keyset pagination.
    pub(crate) fn sort_date(&self) -> i64 {
        self.date_unix.unwrap_or(NULL_DATE_SORT)
    }

    /// Builds an envelope, attaching already-loaded sender/recipient
    /// lists.
    pub(crate) fn into_envelope(
        self,
        from: Vec<Address>,
        to: Vec<Address>,
    ) -> Result<MessageEnvelope, MailError> {
        let remote_id = decode_remote_id(
            self.remote_kind.as_deref(),
            self.remote_uid,
            self.remote_uid_validity,
            self.remote_uidl,
        )?;
        let size_bytes = self
            .size_bytes
            .map(u64::try_from)
            .transpose()
            .map_err(|_| MailError::Database {
                message: "read_message: negative size".to_owned(),
            })?;
        let threading = decode_threading(
            self.message_id,
            self.in_reply_to,
            self.references_json.as_deref(),
        )?;
        Ok(MessageEnvelope {
            id: MessageId::new(self.id),
            mailbox: MailboxId::new(self.mailbox_id),
            remote_id,
            subject: self.subject,
            from,
            to,
            date: self.date_unix.map(UtcTimestamp::from_unix_seconds),
            flags: MessageFlags {
                seen: self.flag_seen,
                answered: self.flag_answered,
                flagged: self.flag_flagged,
                draft: self.flag_draft,
                deleted: self.flag_deleted,
            },
            size_bytes,
            preview: self.preview,
            threading,
        })
    }
}

/// Reconstructs a [`RemoteMessageId`] from its stored columns.
fn decode_remote_id(
    kind: Option<&str>,
    uid: Option<i64>,
    uid_validity: Option<i64>,
    uidl: Option<String>,
) -> Result<Option<RemoteMessageId>, MailError> {
    let corrupt = || MailError::Database {
        message: "read_message: inconsistent remote identity".to_owned(),
    };
    match kind {
        None => Ok(None),
        Some("imap") => {
            let uid = uid.ok_or_else(corrupt)?;
            let uid_validity = uid_validity.ok_or_else(corrupt)?;
            Ok(Some(RemoteMessageId::Imap {
                uid: u32::try_from(uid).map_err(|_| corrupt())?,
                uid_validity: u32::try_from(uid_validity).map_err(|_| corrupt())?,
            }))
        }
        Some("pop3") => Ok(Some(RemoteMessageId::Pop3 {
            uidl: uidl.ok_or_else(corrupt)?,
        })),
        Some(_) => Err(corrupt()),
    }
}

/// Column values for a message's remote identity, ready to bind on
/// insert. Splits a [`RemoteMessageId`] into the nullable columns.
pub(crate) struct RemoteColumns {
    /// `remote_kind` discriminator (`imap`/`pop3`) or `None`.
    pub(crate) kind: Option<&'static str>,
    /// IMAP UID, if applicable.
    pub(crate) uid: Option<i64>,
    /// IMAP `UIDVALIDITY`, if applicable.
    pub(crate) uid_validity: Option<i64>,
    /// POP3 UIDL, if applicable.
    pub(crate) uidl: Option<String>,
}

impl RemoteColumns {
    /// Splits an optional remote identity into storable columns.
    pub(crate) fn from_remote_id(remote: Option<&RemoteMessageId>) -> Self {
        match remote {
            None => Self {
                kind: None,
                uid: None,
                uid_validity: None,
                uidl: None,
            },
            Some(RemoteMessageId::Imap { uid, uid_validity }) => Self {
                kind: Some("imap"),
                uid: Some(i64::from(*uid)),
                uid_validity: Some(i64::from(*uid_validity)),
                uidl: None,
            },
            Some(RemoteMessageId::Pop3 { uidl }) => Self {
                kind: Some("pop3"),
                uid: None,
                uid_validity: None,
                uidl: Some(uidl.clone()),
            },
        }
    }
}

/// Raw `message_recipients` row.
#[derive(sqlx::FromRow)]
struct RecipientRow {
    message_id: i64,
    kind: String,
    display_name: Option<String>,
    email: String,
}

/// Loads `from` and `to` recipients for a set of messages in one query,
/// grouped by message id as `(from, to)`.
///
/// Returns an empty map for an empty id slice without touching the
/// database.
pub(crate) async fn load_recipients(
    pool: &SqlitePool,
    message_ids: &[i64],
) -> Result<HashMap<i64, (Vec<Address>, Vec<Address>)>, MailError> {
    let mut grouped: HashMap<i64, (Vec<Address>, Vec<Address>)> = HashMap::new();
    if message_ids.is_empty() {
        return Ok(grouped);
    }

    let placeholders = vec!["?"; message_ids.len()].join(", ");
    let sql = format!(
        "SELECT message_id, kind, display_name, email FROM message_recipients \
         WHERE message_id IN ({placeholders}) AND kind IN ('from', 'to') \
         ORDER BY message_id, position, id"
    );

    let mut query = sqlx::query_as::<_, RecipientRow>(sqlx::AssertSqlSafe(sql));
    for id in message_ids {
        query = query.bind(id);
    }
    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|error| map_sqlx("load_recipients", error))?;

    for row in rows {
        let address = Address {
            display_name: row.display_name,
            email: EmailAddress::parse(&row.email).map_err(|_| MailError::Database {
                message: "read_recipient: invalid stored address".to_owned(),
            })?,
        };
        let entry = grouped.entry(row.message_id).or_default();
        match row.kind.as_str() {
            "from" => entry.0.push(address),
            "to" => entry.1.push(address),
            _ => {}
        }
    }
    Ok(grouped)
}
