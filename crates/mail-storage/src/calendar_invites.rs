//! Calendar-invitation RSVP repository.
//!
//! Stores only the RSVP state for a `text/calendar` invitation that arrived as
//! a message — the user's response and a lightweight snapshot of the event —
//! keyed one row per message. The client keeps no calendar; the invitation's
//! presentation is re-derived from the message on open. See migration 0018.

use axomail_core::{MailError, MessageId};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error_map::map_sqlx;

/// Columns selected when reading an invitation, in [`InviteRow`] order.
const INVITE_COLUMNS: &str = "id, message_id, uid, summary, organizer, starts_at, ends_at, location, my_status, \
     sequence, cancelled";

/// Identifier of a persisted [`CalendarInvite`] row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InviteId(i64);

impl InviteId {
    /// Wraps a raw database key.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the raw database key.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

/// The user's response to an invitation. Mirrors the `my_status` CHECK domain
/// in migration 0018 exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RsvpStatus {
    /// No response yet.
    None,
    /// The user accepted.
    Accepted,
    /// The user responded tentatively.
    Tentative,
    /// The user declined.
    Declined,
}

impl RsvpStatus {
    /// The persisted token (matches the CHECK domain).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Accepted => "accepted",
            Self::Tentative => "tentative",
            Self::Declined => "declined",
        }
    }

    /// Parses a persisted token back into the enum.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] for an unrecognized token (a corrupt
    /// row; the CHECK constraint prevents writing one).
    pub fn parse(token: &str) -> Result<Self, MailError> {
        match token {
            "none" => Ok(Self::None),
            "accepted" => Ok(Self::Accepted),
            "tentative" => Ok(Self::Tentative),
            "declined" => Ok(Self::Declined),
            _ => Err(MailError::Database {
                message: "unrecognized RSVP status in storage".to_owned(),
            }),
        }
    }
}

/// Fields written when recording or updating an invitation's RSVP state.
#[derive(Debug, Clone)]
pub struct NewInvite {
    /// The message carrying the invitation.
    pub message_id: MessageId,
    /// The event's iCalendar UID.
    pub uid: String,
    /// The event title, if present.
    pub summary: Option<String>,
    /// The organizer's email, if present.
    pub organizer: Option<String>,
    /// Event start as Unix seconds, if resolvable.
    pub starts_at: Option<i64>,
    /// Event end as Unix seconds, if resolvable.
    pub ends_at: Option<i64>,
    /// The event location, if present.
    pub location: Option<String>,
    /// The user's response.
    pub my_status: RsvpStatus,
    /// The REQUEST's SEQUENCE at response time.
    pub sequence: i64,
}

/// A persisted invitation RSVP row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarInvite {
    /// Row identifier.
    pub id: InviteId,
    /// The message carrying the invitation.
    pub message_id: MessageId,
    /// The event's iCalendar UID.
    pub uid: String,
    /// The event title, if stored.
    pub summary: Option<String>,
    /// The organizer's email, if stored.
    pub organizer: Option<String>,
    /// Event start as Unix seconds, if stored.
    pub starts_at: Option<i64>,
    /// Event end as Unix seconds, if stored.
    pub ends_at: Option<i64>,
    /// The event location, if stored.
    pub location: Option<String>,
    /// The user's response.
    pub my_status: RsvpStatus,
    /// The stored SEQUENCE.
    pub sequence: i64,
    /// Whether a CANCEL for this UID has marked the event off.
    pub cancelled: bool,
}

/// Repository over the `calendar_invites` table.
#[derive(Debug)]
pub struct CalendarInviteRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> CalendarInviteRepo<'a> {
    pub(crate) fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Records (or updates) the RSVP state for a message's invitation.
    ///
    /// Keyed on `message_id`: a second call for the same message updates the
    /// response and the event snapshot in place. The `cancelled` flag is left
    /// untouched — only [`mark_cancelled_by_uid`](Self::mark_cancelled_by_uid)
    /// sets it — so a re-response never resurrects a cancelled event.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn upsert(&self, new: &NewInvite) -> Result<CalendarInvite, MailError> {
        let row = sqlx::query_as::<_, InviteRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO calendar_invites
                 (message_id, uid, summary, organizer, starts_at, ends_at, location,
                  my_status, sequence)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (message_id) DO UPDATE SET
                 uid = excluded.uid,
                 summary = excluded.summary,
                 organizer = excluded.organizer,
                 starts_at = excluded.starts_at,
                 ends_at = excluded.ends_at,
                 location = excluded.location,
                 my_status = excluded.my_status,
                 sequence = excluded.sequence
             RETURNING {INVITE_COLUMNS}"
        )))
        .bind(new.message_id.value())
        .bind(new.uid.as_str())
        .bind(new.summary.as_deref())
        .bind(new.organizer.as_deref())
        .bind(new.starts_at)
        .bind(new.ends_at)
        .bind(new.location.as_deref())
        .bind(new.my_status.as_str())
        .bind(new.sequence)
        .fetch_one(self.pool)
        .await
        .map_err(|error| map_sqlx("upsert_calendar_invite", error))?;
        row.into_invite()
    }

    /// Fetches the invitation state for a message, or `None` if none is stored.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure or a bad decode.
    pub async fn get_by_message(
        &self,
        message: MessageId,
    ) -> Result<Option<CalendarInvite>, MailError> {
        let row = sqlx::query_as::<_, InviteRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {INVITE_COLUMNS} FROM calendar_invites WHERE message_id = ?"
        )))
        .bind(message.value())
        .fetch_optional(self.pool)
        .await
        .map_err(|error| map_sqlx("get_calendar_invite", error))?;
        row.map(InviteRow::into_invite).transpose()
    }

    /// Marks every stored invitation for a UID cancelled (a CANCEL arrived).
    /// Returns the number of rows updated.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] on any query failure.
    pub async fn mark_cancelled_by_uid(&self, uid: &str) -> Result<u64, MailError> {
        let result = sqlx::query(
            "UPDATE calendar_invites SET cancelled = 1 WHERE uid = ? AND cancelled = 0",
        )
        .bind(uid)
        .execute(self.pool)
        .await
        .map_err(|error| map_sqlx("cancel_calendar_invites", error))?;
        Ok(result.rows_affected())
    }
}

/// Raw `calendar_invites` row, mapped by column name.
#[derive(sqlx::FromRow)]
struct InviteRow {
    id: i64,
    message_id: i64,
    uid: String,
    summary: Option<String>,
    organizer: Option<String>,
    starts_at: Option<i64>,
    ends_at: Option<i64>,
    location: Option<String>,
    my_status: String,
    sequence: i64,
    cancelled: i64,
}

impl InviteRow {
    fn into_invite(self) -> Result<CalendarInvite, MailError> {
        Ok(CalendarInvite {
            id: InviteId::new(self.id),
            message_id: MessageId::new(self.message_id),
            uid: self.uid,
            summary: self.summary,
            organizer: self.organizer,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            location: self.location,
            my_status: RsvpStatus::parse(&self.my_status)?,
            sequence: self.sequence,
            cancelled: self.cancelled != 0,
        })
    }
}
