//! Emission of an iMIP `REPLY` iCalendar object for an RSVP (RFC 6047 §4).
//!
//! Given the requested event and the responder's participation status, this
//! builds a minimal, valid `METHOD:REPLY` object carrying a single `ATTENDEE`
//! (the responder, with the chosen `PARTSTAT`), the original `UID`/`SEQUENCE`,
//! the `ORGANIZER`, and the event's start/end echoed verbatim.

use super::CalendarEvent;

/// Product identifier written into the reply's `PRODID`.
const PRODID: &str = "-//axomail//iMIP 1.0//EN";

/// The responder's inputs to a reply.
#[derive(Debug, Clone)]
pub struct ReplyParams {
    /// The responding attendee's email address.
    pub responder_email: String,
    /// The responding attendee's display name, if known (written as `CN`).
    pub responder_name: Option<String>,
    /// The `PARTSTAT` value: `ACCEPTED`, `TENTATIVE`, or `DECLINED`.
    pub partstat: String,
    /// `DTSTAMP` as Unix seconds (the moment the reply is generated). Passed in
    /// rather than read from the clock so emission stays pure and testable.
    pub dtstamp_unix: i64,
}

/// Builds the `text/calendar` body of an iMIP reply for `event`.
///
/// The result uses CRLF line endings and RFC 5545 line folding, ready to be
/// attached as a `text/calendar; method=REPLY` MIME part.
#[must_use]
pub fn build_reply(event: &CalendarEvent, params: &ReplyParams) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("BEGIN:VCALENDAR".to_owned());
    lines.push("VERSION:2.0".to_owned());
    lines.push(format!("PRODID:{PRODID}"));
    lines.push("METHOD:REPLY".to_owned());
    lines.push("BEGIN:VEVENT".to_owned());
    lines.push(format!("UID:{}", event.uid));
    lines.push(format!("SEQUENCE:{}", event.sequence));
    lines.push(format!("DTSTAMP:{}", ical_utc_stamp(params.dtstamp_unix)));

    // Echo the times verbatim so the organizer's client matches the instance.
    if let Some(dtstart) = &event.dtstart_line {
        lines.push(dtstart.clone());
    }
    if let Some(dtend) = &event.dtend_line {
        lines.push(dtend.clone());
    }
    if let Some(summary) = &event.summary {
        lines.push(format!("SUMMARY:{}", escape_text(summary)));
    }
    if let Some(organizer) = &event.organizer {
        let cn = organizer
            .name
            .as_deref()
            .map(|name| format!(";CN={}", quote_param(name)))
            .unwrap_or_default();
        lines.push(format!("ORGANIZER{cn}:mailto:{}", organizer.email));
    }

    let responder_cn = params
        .responder_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!(";CN={}", quote_param(name)))
        .unwrap_or_default();
    lines.push(format!(
        "ATTENDEE{responder_cn};PARTSTAT={}:mailto:{}",
        params.partstat, params.responder_email,
    ));

    lines.push("END:VEVENT".to_owned());
    lines.push("END:VCALENDAR".to_owned());

    let mut out = String::new();
    for line in &lines {
        out.push_str(&fold_line(line));
        out.push_str("\r\n");
    }
    out
}

/// Formats Unix seconds as an iCalendar UTC timestamp (`YYYYMMDDTHHMMSSZ`).
fn ical_utc_stamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs_of_day = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Inverse of [`super::days_from_civil`]: the civil date for a day count since
/// the Unix epoch (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Escapes an iCalendar `TEXT` value: backslash, semicolon, comma, and newline.
fn escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// Wraps a parameter value in double quotes, dropping any embedded quotes,
/// semicolons, colons, or commas that a quoted param value may not carry.
fn quote_param(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            '"' | ';' | ':' | ',' => ' ',
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect();
    format!("\"{}\"", cleaned.trim())
}

/// Folds a content line to at most 75 octets per physical line, continuing with
/// CRLF + a single space, without splitting a multi-byte UTF-8 character.
fn fold_line(line: &str) -> String {
    const LIMIT: usize = 75;
    let mut out = String::with_capacity(line.len());
    let mut octets = 0usize;
    for ch in line.chars() {
        let width = ch.len_utf8();
        if octets + width > LIMIT {
            out.push_str("\r\n ");
            // The continued line starts with the leading space already counted.
            octets = 1;
        }
        out.push(ch);
        octets += width;
    }
    out
}
