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

/// Trims a display name and drops it when empty.
fn clean_name(name: Option<&str>) -> Option<String> {
    let trimmed = name?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
