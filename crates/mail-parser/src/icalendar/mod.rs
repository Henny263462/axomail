//! iCalendar (RFC 5545) extraction and emission for iMIP (RFC 6047).
//!
//! This is the mail-side of calendar invitations only: extracting a
//! `text/calendar` invitation from a parsed message ([`extract_invitation`]),
//! and building an iMIP `REPLY` object for an RSVP ([`build_reply`]). It is a
//! deliberately small, dependency-free subset — no calendar storage, no
//! recurrence expansion, no timezone database (a named `TZID` is preserved for
//! display but not resolved to an absolute offset).

mod emit;
mod parse;

use crate::model::ParsedMessage;

pub use emit::{ReplyParams, build_reply};
pub use parse::parse_calendar;

/// The iMIP method (iTIP transaction type) carried by the `METHOD` property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarMethod {
    /// A meeting invitation seeking an RSVP.
    Request,
    /// An attendee's RSVP to a request.
    Reply,
    /// The organizer cancelling a previously requested event.
    Cancel,
    /// A one-way publish (no RSVP expected).
    Publish,
    /// An attendee's counter-proposal.
    Counter,
    /// A refresh request.
    Refresh,
    /// Any other method token, preserved verbatim (uppercased).
    Other(String),
}

impl CalendarMethod {
    /// A stable lowercase slug for the DTO / UI (`"request"`, `"cancel"`, …).
    #[must_use]
    pub fn slug(&self) -> String {
        match self {
            Self::Request => "request".to_owned(),
            Self::Reply => "reply".to_owned(),
            Self::Cancel => "cancel".to_owned(),
            Self::Publish => "publish".to_owned(),
            Self::Counter => "counter".to_owned(),
            Self::Refresh => "refresh".to_owned(),
            Self::Other(other) => other.to_ascii_lowercase(),
        }
    }
}

/// A calendar user address: an email plus an optional common name (`CN`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalAddress {
    /// The bare email address (the `mailto:` scheme stripped).
    pub email: String,
    /// The `CN` (common name) parameter, if the line carried one.
    pub name: Option<String>,
}

/// One `ATTENDEE` line: address, name, and participation status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalAttendee {
    /// The bare email address (the `mailto:` scheme stripped).
    pub email: String,
    /// The `CN` (common name) parameter, if present.
    pub name: Option<String>,
    /// The `PARTSTAT` parameter, uppercased (e.g. `NEEDS-ACTION`).
    pub partstat: Option<String>,
}

/// The timezone attached to a date-time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TzSpec {
    /// UTC (a trailing `Z`); the only case with an exact absolute instant.
    Utc,
    /// A named `TZID` (e.g. `Europe/Berlin`), preserved for display. Without a
    /// timezone database the absolute instant is approximated as if UTC.
    Named(String),
    /// A floating (no-timezone) local time.
    Floating,
}

/// A parsed iCalendar `DATE` or `DATE-TIME`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalDateTime {
    /// Four-digit year.
    pub year: i32,
    /// Month, 1–12.
    pub month: u32,
    /// Day of month, 1–31.
    pub day: u32,
    /// Hour, 0–23 (0 for an all-day date).
    pub hour: u32,
    /// Minute, 0–59.
    pub minute: u32,
    /// Second, 0–59.
    pub second: u32,
    /// Whether this is an all-day `VALUE=DATE` (no time component).
    pub date_only: bool,
    /// The timezone specification.
    pub tz: TzSpec,
}

impl CalDateTime {
    /// The instant as Unix seconds.
    ///
    /// Exact for a UTC (`Z`) time; for a named or floating time the wall-clock
    /// is treated as if it were UTC (a documented approximation, since no
    /// timezone database is bundled). The value is used only for storage and
    /// coarse ordering, never for the displayed time, which shows the original
    /// wall-clock with its timezone label.
    #[must_use]
    pub fn to_unix_seconds(&self) -> i64 {
        let days = days_from_civil(self.year, self.month, self.day);
        days * 86_400
            + i64::from(self.hour) * 3_600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    /// A short timezone label for display: `UTC`, the `TZID`, or `None` for a
    /// floating local time.
    #[must_use]
    pub fn tz_label(&self) -> Option<String> {
        match &self.tz {
            TzSpec::Utc => Some("UTC".to_owned()),
            TzSpec::Named(id) => Some(id.clone()),
            TzSpec::Floating => None,
        }
    }
}

/// A single event extracted from a `VEVENT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    /// The globally-unique `UID`.
    pub uid: String,
    /// The `SUMMARY` (title), unescaped.
    pub summary: Option<String>,
    /// The `ORGANIZER`.
    pub organizer: Option<CalAddress>,
    /// Every `ATTENDEE`, in document order.
    pub attendees: Vec<CalAttendee>,
    /// The parsed `DTSTART`, if present.
    pub dtstart: Option<CalDateTime>,
    /// The parsed `DTEND`, if present.
    pub dtend: Option<CalDateTime>,
    /// The raw, unfolded `DTSTART` content line (echoed verbatim in a reply so
    /// its `TZID`/value survive round-trips exactly).
    pub dtstart_line: Option<String>,
    /// The raw, unfolded `DTEND` content line.
    pub dtend_line: Option<String>,
    /// The `LOCATION`, unescaped.
    pub location: Option<String>,
    /// The `DESCRIPTION` (agenda), unescaped.
    pub description: Option<String>,
    /// The `SEQUENCE` revision number (defaults to 0).
    pub sequence: i64,
    /// The `STATUS`, uppercased (e.g. `CANCELLED`).
    pub status: Option<String>,
}

impl CalendarEvent {
    /// Whether the event carries `STATUS:CANCELLED`.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.status.as_deref() == Some("CANCELLED")
    }

    /// The email of the attendee matching any of `self_emails` (lowercased),
    /// preferring the exact address the organizer listed for the responder.
    #[must_use]
    pub fn attendee_matching(&self, self_emails: &[String]) -> Option<&CalAttendee> {
        self.attendees.iter().find(|attendee| {
            self_emails
                .iter()
                .any(|mine| mine.eq_ignore_ascii_case(&attendee.email))
        })
    }
}

/// An extracted iMIP invitation: the `METHOD` and its first `VEVENT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// The iMIP method.
    pub method: CalendarMethod,
    /// The first event carried by the object.
    pub event: CalendarEvent,
}

/// Extracts the first iMIP invitation from a parsed message's `text/calendar`
/// part, if the message carries one.
///
/// Returns `None` for a message with no calendar part, or a calendar object
/// with no `METHOD` (not an iMIP message) or no usable `VEVENT`.
#[must_use]
pub fn extract_invitation(message: &ParsedMessage) -> Option<Invitation> {
    let part = message
        .attachments
        .iter()
        .find(|part| part.mime_type == "text/calendar")?;
    let text = String::from_utf8_lossy(&part.bytes);
    parse_calendar(&text)
}

/// Builds a human-readable "when" line from a start and optional end, appending
/// the timezone label (e.g. `20 Jul 2026, 13:00 – 14:00 (Europe/Berlin)`).
///
/// Returns `None` when there is no start time. An all-day event renders the
/// date without a clock time.
#[must_use]
pub fn format_when(start: Option<&CalDateTime>, end: Option<&CalDateTime>) -> Option<String> {
    let start = start?;
    let mut out = format_date(start);
    if !start.date_only {
        out.push_str(", ");
        out.push_str(&format_time(start));
        if let Some(end) = end.filter(|end| !end.date_only && same_day(start, end)) {
            out.push_str(" – ");
            out.push_str(&format_time(end));
        }
    }
    if let Some(label) = start.tz_label() {
        out.push_str(" (");
        out.push_str(&label);
        out.push(')');
    }
    Some(out)
}

/// `DD Mon YYYY` for a date.
fn format_date(dt: &CalDateTime) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS
        .get((dt.month.max(1) - 1) as usize)
        .copied()
        .unwrap_or("???");
    format!("{:02} {} {:04}", dt.day, month, dt.year)
}

/// `HH:MM` for a time.
fn format_time(dt: &CalDateTime) -> String {
    format!("{:02}:{:02}", dt.hour, dt.minute)
}

/// Whether two date-times fall on the same calendar day.
fn same_day(a: &CalDateTime, b: &CalDateTime) -> bool {
    (a.year, a.month, a.day) == (b.year, b.month, b.day)
}

/// Days from the Unix epoch (1970-01-01) to a proleptic-Gregorian date, by
/// Howard Hinnant's `days_from_civil` algorithm. Valid across the whole range
/// of dates a calendar invitation carries.
pub(super) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + doe - 719_468
}
