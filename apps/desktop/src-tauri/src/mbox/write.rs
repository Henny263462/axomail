//! mbox serialization (writer side).
//!
//! Serializes one message at a time into the standard **mboxrd** container
//! form so a folder can be streamed to disk without ever holding the whole
//! archive in memory: each message is framed by a `From ` postmark line, its
//! body is `>From`-escaped, and a blank line separates it from the next.
//!
//! mboxrd escaping rule (the inverse of [`crate::mbox::read`]): any body line
//! matching `^>*From ` gains one extra leading `>`, so no line of message
//! content can be mistaken for a postmark on read-back. This is the escaping
//! Thunderbird, mutt, and Apple Mail all use, so the output imports cleanly
//! elsewhere and round-trips through our own reader byte-for-byte.

use std::io::{self, Write};

/// Serializes one message into `out` as a single mbox record: a `From `
/// postmark (using `sender` and `date`), the `>From`-escaped `raw` bytes
/// terminated by a newline, and a trailing blank-line separator.
///
/// `sender` is the envelope sender for the postmark (an email address; only
/// its first whitespace-delimited token is used, falling back to
/// `MAILER-DAEMON`). `date` is the postmark time in Unix seconds; `None`
/// formats the epoch. The postmark is informational only — the reader derives
/// the real envelope by parsing the message — so neither value affects
/// round-trip fidelity.
///
/// # Errors
///
/// Propagates any IO error from writing to `out`.
pub fn write_message<W: Write>(
    out: &mut W,
    sender: &str,
    date: Option<i64>,
    raw: &[u8],
) -> io::Result<()> {
    write_postmark(out, sender, date)?;
    write_escaped_body(out, raw)?;
    // Blank-line separator before the next postmark (mboxrd convention).
    out.write_all(b"\n")
}

/// Writes the `From <sender> <asctime>` postmark line.
fn write_postmark<W: Write>(out: &mut W, sender: &str, date: Option<i64>) -> io::Result<()> {
    out.write_all(b"From ")?;
    out.write_all(postmark_sender(sender).as_bytes())?;
    out.write_all(b" ")?;
    out.write_all(asctime_utc(date.unwrap_or(0)).as_bytes())?;
    out.write_all(b"\n")
}

/// The postmark sender token: the first whitespace-delimited field of
/// `sender` (an email address carries no spaces), or `MAILER-DAEMON` when
/// blank. Guarantees the postmark line stays single-token and never smuggles a
/// space or newline that would fracture the framing.
fn postmark_sender(sender: &str) -> &str {
    match sender.split_whitespace().next() {
        Some(token) if !token.is_empty() => token,
        _ => "MAILER-DAEMON",
    }
}

/// Writes `raw` with mboxrd `>From` escaping, guaranteeing the output ends
/// with exactly one newline so the following separator sits on its own line.
fn write_escaped_body<W: Write>(out: &mut W, raw: &[u8]) -> io::Result<()> {
    if raw.is_empty() {
        return out.write_all(b"\n");
    }
    let mut start = 0;
    while start < raw.len() {
        let end = match memchr(b'\n', &raw[start..]) {
            Some(offset) => start + offset + 1,
            None => raw.len(),
        };
        let line = &raw[start..end];
        write_body_line(out, line)?;
        // A final line without its own newline is terminated so the blank-line
        // separator lands cleanly.
        if !line.ends_with(b"\n") {
            out.write_all(b"\n")?;
        }
        start = end;
    }
    Ok(())
}

/// Writes one body line, prepending a `>` when it would otherwise read as a
/// postmark or an already-escaped postmark (`^>*From `).
fn write_body_line<W: Write>(out: &mut W, line: &[u8]) -> io::Result<()> {
    if needs_escape(line) {
        out.write_all(b">")?;
    }
    out.write_all(line)
}

/// Whether a line matches `^>*From ` (zero or more `>` then `From `), the
/// mboxrd family of lines that must be escaped so read-back cannot confuse
/// them with a postmark.
fn needs_escape(line: &[u8]) -> bool {
    let mut i = 0;
    while i < line.len() && line[i] == b'>' {
        i += 1;
    }
    line[i..].starts_with(b"From ")
}

/// First index of `needle` in `haystack` (a tiny dependency-free `memchr`).
fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&byte| byte == needle)
}

/// Month abbreviations for [`asctime_utc`].
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Weekday abbreviations for [`asctime_utc`], Sunday first.
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Formats a Unix timestamp as a POSIX `asctime` string in UTC, e.g.
/// `Thu Jan 01 00:00:00 1970`. Locale-independent (fixed English
/// abbreviations) and dependency-free; used only for the mbox postmark, which
/// the reader discards.
fn asctime_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday (index 4); shift the epoch-day count onto the
    // Sunday-first weekday table.
    let weekday = (days.rem_euclid(7) + 4).rem_euclid(7) as usize;
    let hour = secs / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    format!(
        "{} {} {:02} {:02}:{:02}:{:02} {}",
        WEEKDAYS[weekday],
        MONTHS[(month - 1) as usize],
        day,
        hour,
        minute,
        second,
        year,
    )
}

/// Converts a count of days since the Unix epoch to a `(year, month, day)`
/// civil date (Howard Hinnant's algorithm), valid across the whole i64 range.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn write_one(sender: &str, date: Option<i64>, raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_message(&mut out, sender, date, raw).unwrap();
        out
    }

    #[test]
    fn epoch_formats_as_asctime() {
        assert_eq!(asctime_utc(0), "Thu Jan 01 00:00:00 1970");
    }

    #[test]
    fn a_known_date_formats_correctly() {
        // 2021-01-01 00:00:00 UTC = 1609459200; that day was a Friday.
        assert_eq!(asctime_utc(1_609_459_200), "Fri Jan 01 00:00:00 2021");
    }

    #[test]
    fn postmark_carries_the_sender_and_a_date() {
        let out = write_one("alice@example.org", Some(0), b"Body\r\n");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("From alice@example.org Thu Jan 01 00:00:00 1970\n"));
    }

    #[test]
    fn blank_sender_falls_back_to_mailer_daemon() {
        let out = write_one("   ", Some(0), b"x\n");
        assert!(
            String::from_utf8(out)
                .unwrap()
                .starts_with("From MAILER-DAEMON ")
        );
    }

    #[test]
    fn from_lines_in_the_body_are_escaped() {
        let raw = b"Header: v\r\n\r\nFrom the desk of Alice\r\n>From already\r\n";
        let out = write_one("a@b.c", Some(0), raw);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\n>From the desk of Alice\r\n"));
        assert!(text.contains("\n>>From already\r\n"));
    }

    #[test]
    fn a_real_from_header_is_not_escaped() {
        // "From:" (a header) is not "From " (a postmark) and must be untouched.
        let raw = b"From: alice@example.org\r\n\r\nbody\r\n";
        let out = write_one("a@b.c", Some(0), raw);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\nFrom: alice@example.org\r\n"));
        assert!(!text.contains(">From:"));
    }

    #[test]
    fn output_is_newline_terminated_with_a_blank_separator() {
        // A body without a trailing newline is terminated, then a blank line.
        let out = write_one("a@b.c", Some(0), b"no newline");
        assert!(out.ends_with(b"no newline\n\n"));
    }
}
