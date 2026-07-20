//! Dependency-free parser for the iCalendar (RFC 5545) subset iMIP needs.
//!
//! The full grammar is large; this parses only what an invitation card and an
//! RSVP reply require: the `METHOD`, and the first `VEVENT`'s `UID`, `SUMMARY`,
//! `DTSTART`/`DTEND` (with `TZID`), `ORGANIZER`, `ATTENDEE`s, `LOCATION`,
//! `DESCRIPTION`, `SEQUENCE`, and `STATUS`. Line unfolding, quoted parameters,
//! and `TEXT`-value escaping are all handled; everything else is ignored.

use super::{
    CalAddress, CalAttendee, CalDateTime, CalendarEvent, CalendarMethod, Invitation, TzSpec,
};

/// Parses an iCalendar object, returning the first `VEVENT` together with the
/// enclosing `VCALENDAR`'s `METHOD`.
///
/// Returns `None` when the input carries no `METHOD` (it is then not an iMIP
/// message) or no `VEVENT` with a `UID`. Parsing is tolerant: unknown
/// properties and components are skipped, and a malformed line is dropped
/// rather than failing the whole object.
#[must_use]
pub fn parse_calendar(text: &str) -> Option<Invitation> {
    let lines = unfold(text);
    let mut stack: Vec<String> = Vec::new();
    let mut method: Option<CalendarMethod> = None;
    let mut builder: Option<EventBuilder> = None;
    let mut event: Option<CalendarEvent> = None;

    for line in &lines {
        let Some(content) = parse_line(line) else {
            continue;
        };
        match content.name.as_str() {
            "BEGIN" => {
                let component = content.value.trim().to_ascii_uppercase();
                if component == "VEVENT" && builder.is_none() && event.is_none() {
                    builder = Some(EventBuilder::default());
                }
                stack.push(component);
                continue;
            }
            "END" => {
                let component = content.value.trim().to_ascii_uppercase();
                if component == "VEVENT"
                    && let Some(done) = builder.take()
                {
                    event = done.build();
                }
                stack.pop();
                continue;
            }
            _ => {}
        }

        let inside_event = stack.last().map(String::as_str) == Some("VEVENT");
        if content.name == "METHOD" && !inside_event {
            method.get_or_insert_with(|| CalendarMethod::parse(content.value.trim()));
        } else if inside_event && let Some(builder) = builder.as_mut() {
            builder.apply(&content);
        }
    }

    Some(Invitation {
        method: method?,
        event: event?,
    })
}

/// Accumulates the properties of the `VEVENT` currently being parsed.
#[derive(Default)]
struct EventBuilder {
    uid: Option<String>,
    summary: Option<String>,
    organizer: Option<CalAddress>,
    attendees: Vec<CalAttendee>,
    dtstart: Option<CalDateTime>,
    dtend: Option<CalDateTime>,
    dtstart_line: Option<String>,
    dtend_line: Option<String>,
    location: Option<String>,
    description: Option<String>,
    sequence: i64,
    status: Option<String>,
}

impl EventBuilder {
    /// Folds one content line into the event being built.
    fn apply(&mut self, line: &ContentLine) {
        match line.name.as_str() {
            "UID" => self.uid = Some(line.value.trim().to_owned()),
            "SUMMARY" => self.summary = Some(unescape_text(&line.value)),
            "LOCATION" => self.location = Some(unescape_text(&line.value)),
            "DESCRIPTION" => self.description = Some(unescape_text(&line.value)),
            "SEQUENCE" => {
                if let Ok(sequence) = line.value.trim().parse::<i64>() {
                    self.sequence = sequence;
                }
            }
            "STATUS" => self.status = Some(line.value.trim().to_ascii_uppercase()),
            "ORGANIZER" => self.organizer = Some(cal_address(line)),
            "ATTENDEE" => self.attendees.push(cal_attendee(line)),
            "DTSTART" => {
                self.dtstart = parse_datetime(line);
                self.dtstart_line = Some(line.raw.clone());
            }
            "DTEND" => {
                self.dtend = parse_datetime(line);
                self.dtend_line = Some(line.raw.clone());
            }
            _ => {}
        }
    }

    /// Produces the event, requiring at least a `UID`.
    fn build(self) -> Option<CalendarEvent> {
        Some(CalendarEvent {
            uid: self.uid?,
            summary: self.summary,
            organizer: self.organizer,
            attendees: self.attendees,
            dtstart: self.dtstart,
            dtend: self.dtend,
            dtstart_line: self.dtstart_line,
            dtend_line: self.dtend_line,
            location: self.location,
            description: self.description,
            sequence: self.sequence,
            status: self.status,
        })
    }
}

impl CalendarMethod {
    /// Maps a `METHOD` token onto the enum, preserving unknown tokens verbatim
    /// (uppercased).
    fn parse(token: &str) -> Self {
        match token.to_ascii_uppercase().as_str() {
            "REQUEST" => Self::Request,
            "REPLY" => Self::Reply,
            "CANCEL" => Self::Cancel,
            "PUBLISH" => Self::Publish,
            "COUNTER" => Self::Counter,
            "REFRESH" => Self::Refresh,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// One parsed content line: the (uppercased) property name, its parameters,
/// the raw value, and the whole unfolded line (kept so `DTSTART`/`DTEND` can be
/// echoed verbatim in a reply).
struct ContentLine {
    name: String,
    params: Vec<(String, String)>,
    value: String,
    raw: String,
}

impl ContentLine {
    /// Returns the first value of a parameter (case-insensitive name).
    fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

/// Splits a content line into name, parameters, and value.
///
/// The value begins after the first unquoted colon; the name and any
/// parameters (`;`-separated, `=`-valued, optionally double-quoted) precede it.
fn parse_line(line: &str) -> Option<ContentLine> {
    if line.trim().is_empty() {
        return None;
    }
    let colon = unquoted_index(line, ':')?;
    let head = &line[..colon];
    let value = line[colon + 1..].to_owned();

    let mut segments = split_unquoted(head, ';');
    let name = segments.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = segments
        .filter_map(|segment| {
            let (key, raw) = segment.split_once('=')?;
            Some((key.trim().to_ascii_uppercase(), unquote(raw.trim())))
        })
        .collect();

    Some(ContentLine {
        name,
        params,
        value,
        raw: line.to_owned(),
    })
}

/// Builds a calendar address (`ORGANIZER`) from a `mailto:` value plus `CN`.
fn cal_address(line: &ContentLine) -> CalAddress {
    CalAddress {
        email: strip_mailto(line.value.trim()),
        name: line
            .param("CN")
            .map(str::to_owned)
            .filter(|n| !n.is_empty()),
    }
}

/// Builds an attendee from a `mailto:` value plus `CN`/`PARTSTAT`.
fn cal_attendee(line: &ContentLine) -> CalAttendee {
    CalAttendee {
        email: strip_mailto(line.value.trim()),
        name: line
            .param("CN")
            .map(str::to_owned)
            .filter(|n| !n.is_empty()),
        partstat: line
            .param("PARTSTAT")
            .map(|value| value.to_ascii_uppercase()),
    }
}

/// Removes a leading, case-insensitive `mailto:` scheme from a calendar address.
fn strip_mailto(value: &str) -> String {
    value
        .strip_prefix("mailto:")
        .or_else(|| value.strip_prefix("MAILTO:"))
        .or_else(|| {
            value
                .get(..7)
                .filter(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
                .map(|_| &value[7..])
        })
        .unwrap_or(value)
        .trim()
        .to_owned()
}

/// Parses a `DATE`/`DATE-TIME` value with its `TZID`/`VALUE` parameters.
fn parse_datetime(line: &ContentLine) -> Option<CalDateTime> {
    let value = line.value.trim();
    let date_only = line
        .param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
        || !value.contains('T');
    let is_utc = value.ends_with('Z') || value.ends_with('z');
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return None;
    }
    let year = digits.get(0..4)?.parse().ok()?;
    let month = digits.get(4..6)?.parse().ok()?;
    let day = digits.get(6..8)?.parse().ok()?;
    let (hour, minute, second) = if date_only {
        (0, 0, 0)
    } else {
        (
            digits.get(8..10).and_then(|s| s.parse().ok()).unwrap_or(0),
            digits.get(10..12).and_then(|s| s.parse().ok()).unwrap_or(0),
            digits.get(12..14).and_then(|s| s.parse().ok()).unwrap_or(0),
        )
    };
    let tz = if is_utc {
        TzSpec::Utc
    } else if let Some(tzid) = line.param("TZID").filter(|t| !t.is_empty()) {
        TzSpec::Named(tzid.to_owned())
    } else {
        TzSpec::Floating
    };
    Some(CalDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        date_only,
        tz,
    })
}

/// Unfolds RFC 5545 folded lines: a line beginning with a space or tab is a
/// continuation of the previous one (the single leading whitespace is dropped).
fn unfold(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let mut lines: Vec<String> = Vec::new();
    for raw in normalized.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix([' ', '\t'])
            && let Some(last) = lines.last_mut()
        {
            last.push_str(rest);
        } else {
            lines.push(raw.to_owned());
        }
    }
    lines
}

/// Returns the byte index of the first `needle` not inside a double-quoted run.
fn unquoted_index(haystack: &str, needle: char) -> Option<usize> {
    let mut in_quote = false;
    for (index, ch) in haystack.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            c if c == needle && !in_quote => return Some(index),
            _ => {}
        }
    }
    None
}

/// Splits on `separator`, ignoring separators inside double-quoted runs.
fn split_unquoted(input: &str, separator: char) -> impl Iterator<Item = &str> {
    let mut boundaries = vec![0usize];
    let mut in_quote = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            c if c == separator && !in_quote => {
                boundaries.push(index);
            }
            _ => {}
        }
    }
    boundaries.push(input.len());
    (0..boundaries.len() - 1).map(move |i| {
        let start = boundaries[i];
        let end = boundaries[i + 1];
        // Skip the separator char itself for every segment after the first.
        let start = if i == 0 { start } else { start + 1 };
        &input[start..end]
    })
}

/// Removes one pair of surrounding double quotes from a parameter value.
fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

/// Unescapes an iCalendar `TEXT` value (`\n`, `\,`, `\;`, `\\`).
fn unescape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}
