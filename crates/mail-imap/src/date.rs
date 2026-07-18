//! Minimal RFC 5322 date parsing for `ENVELOPE` date fields.
//!
//! The IMAP `ENVELOPE` carries the message `Date:` header verbatim. This
//! parser turns the common forms into a [`UtcTimestamp`] without pulling
//! in a date-time dependency; anything it cannot parse yields `None`.

use axomail_core::UtcTimestamp;

/// Parses an RFC 5322 date value into a UTC timestamp, or `None` if it is
/// absent or not parseable.
#[must_use]
pub(crate) fn parse(raw: &[u8]) -> Option<UtcTimestamp> {
    let text = String::from_utf8_lossy(raw);
    // Drop an optional leading day-of-week ("Tue, ").
    let without_dow = text.split(',').next_back().unwrap_or(&text);
    let mut parts = without_dow.split_whitespace();

    let day: i64 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let year = parse_year(parts.next()?)?;
    let (hour, minute, second) = parse_time(parts.next()?)?;
    let offset = parts.next().map_or(0, parse_zone);

    if !(1..=31).contains(&day) || !(0..=23).contains(&hour) || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset;
    Some(UtcTimestamp::from_unix_seconds(seconds))
}

/// Maps a three-letter English month abbreviation to its number.
fn month_number(name: &str) -> Option<i64> {
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = name.to_ascii_lowercase();
    months
        .iter()
        .position(|m| lower.starts_with(m))
        .map(|idx| idx as i64 + 1)
}

/// Parses a 2- or 4-digit year, applying the RFC 2822 two-digit rules.
fn parse_year(token: &str) -> Option<i64> {
    let value: i64 = token.parse().ok()?;
    let normalized = match token.len() {
        1 | 2 if value < 50 => value + 2000,
        1 | 2 => value + 1900,
        3 => value + 1900,
        _ => value,
    };
    Some(normalized)
}

/// Parses `HH:MM` or `HH:MM:SS` into its components.
fn parse_time(token: &str) -> Option<(i64, i64, i64)> {
    let mut fields = token.split(':');
    let hour: i64 = fields.next()?.parse().ok()?;
    let minute: i64 = fields.next()?.parse().ok()?;
    let second: i64 = match fields.next() {
        Some(value) => value.parse().ok()?,
        None => 0,
    };
    Some((hour, minute, second))
}

/// Parses a numeric (`+HHMM`) or named zone into an offset in seconds.
///
/// Unknown zones are treated as UTC, matching the RFC 5322 handling of
/// the obsolete `-0000` "unknown" zone.
fn parse_zone(token: &str) -> i64 {
    let bytes = token.as_bytes();
    if let Some(&sign) = bytes.first()
        && (sign == b'+' || sign == b'-')
        && bytes.len() == 5
        && let (Ok(hh), Ok(mm)) = (token[1..3].parse::<i64>(), token[3..5].parse::<i64>())
    {
        let magnitude = hh * 3_600 + mm * 60;
        return if sign == b'-' { -magnitude } else { magnitude };
    }
    match token.to_ascii_uppercase().as_str() {
        "EST" => -5 * 3_600,
        "EDT" => -4 * 3_600,
        "CST" => -6 * 3_600,
        "CDT" => -5 * 3_600,
        "MST" => -7 * 3_600,
        "MDT" => -6 * 3_600,
        "PST" => -8 * 3_600,
        "PDT" => -7 * 3_600,
        _ => 0,
    }
}

/// Days from the Unix epoch for a proleptic-Gregorian date (Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(raw: &str) -> Option<i64> {
        parse(raw.as_bytes()).map(UtcTimestamp::unix_seconds)
    }

    #[test]
    fn parses_full_rfc5322_date() {
        // 2021-01-15 00:00:00 UTC == 1_610_668_800.
        assert_eq!(ts("Fri, 15 Jan 2021 00:00:00 +0000"), Some(1_610_668_800));
    }

    #[test]
    fn epoch_is_zero() {
        assert_eq!(ts("Thu, 01 Jan 1970 00:00:00 +0000"), Some(0));
        assert_eq!(ts("1 Jan 1970 00:00:00 GMT"), Some(0));
    }

    #[test]
    fn applies_timezone_offset() {
        // 01:00:00 +0100 is midnight UTC.
        assert_eq!(ts("Thu, 01 Jan 1970 01:00:00 +0100"), Some(0));
        // 23:00:00 -0100 on the previous day is also midnight UTC.
        assert_eq!(ts("Wed, 31 Dec 1969 23:00:00 -0100"), Some(0));
    }

    #[test]
    fn handles_missing_seconds_and_zone() {
        assert_eq!(ts("01 Jan 1970 00:00"), Some(0));
    }

    #[test]
    fn handles_named_zone() {
        // 1970-01-01 00:00:00 EST is 05:00:00 UTC.
        assert_eq!(ts("Thu, 01 Jan 1970 00:00:00 EST"), Some(5 * 3_600));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(ts("not a date"), None);
        assert_eq!(ts(""), None);
        assert_eq!(ts("32 Jan 2021 00:00:00 +0000"), None);
    }
}
