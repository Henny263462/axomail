//! Conversion from `mail_parser` address values to core [`Address`] lists.
//!
//! `mail_parser` addresses may be malformed (missing or syntactically invalid
//! email parts). Rather than failing the whole message, invalid entries are
//! dropped and recorded as redacted warnings, matching the tolerant contract
//! of [`parse_message`](crate::parse_message).

use axomail_core::{Address, EmailAddress};
use mail_parser::Address as RawAddress;

use crate::redact::redact_address;

/// Collects every valid address from an optional header value.
///
/// Works for both address lists and RFC 5322 groups. Entries without a
/// parseable email are skipped; each drop appends a redacted note to
/// `warnings`.
pub(crate) fn collect_addresses(
    header: Option<&RawAddress<'_>>,
    field: &str,
    warnings: &mut Vec<String>,
) -> Vec<Address> {
    let Some(header) = header else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for addr in header.iter() {
        match addr.address() {
            Some(raw) => match EmailAddress::parse(raw.trim()) {
                Ok(email) => out.push(Address {
                    display_name: clean_name(addr.name()),
                    email,
                }),
                Err(_) => warnings.push(format!(
                    "dropped unparseable {field} address {}",
                    redact_address(raw)
                )),
            },
            None => warnings.push(format!("dropped {field} entry without an email address")),
        }
    }
    out
}

/// Collects the first valid address from an optional header value.
///
/// Warnings for any invalid entries encountered before the first valid one
/// are still recorded.
pub(crate) fn first_address(
    header: Option<&RawAddress<'_>>,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<Address> {
    collect_addresses(header, field, warnings)
        .into_iter()
        .next()
}

/// Extracts the email addresses from a raw address-list header value (the
/// unparsed text after the colon), such as `Disposition-Notification-To`.
///
/// The `mail_parser` default configuration does not type this header, and
/// switching its config to do so drops MIME-body parsing (a documented
/// `mail_parser` builder quirk), so the addresses are recovered here instead.
/// Comma-separated entries are split; a `Name <addr>` form is reduced to the
/// bracketed address, a bare token is used as-is; each is validated and
/// invalid entries are dropped. A quoted display name containing a comma is
/// not handled (rare for this header) — such an entry is simply skipped.
pub(crate) fn addresses_from_raw(raw: &str) -> Vec<Address> {
    raw.split(',')
        .filter_map(|entry| {
            let token = match (entry.find('<'), entry.find('>')) {
                (Some(open), Some(close)) if close > open + 1 => &entry[open + 1..close],
                _ => entry,
            };
            EmailAddress::parse(token.trim())
                .ok()
                .map(Address::from_email)
        })
        .collect()
}

/// Trims a display name and drops it when empty.
fn clean_name(name: Option<&str>) -> Option<String> {
    let trimmed = name?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
