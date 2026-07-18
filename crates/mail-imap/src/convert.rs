//! Conversions between async-imap/imap-proto types and core models.

use async_imap::imap_proto::types::Address as ImapAddress;
use async_imap::types::{Fetch, Flag};
use axomail_core::{Address, EmailAddress, MessageFlags, RemoteEnvelope, RemoteMessageId};

use crate::{date, rfc2047};

/// Reads the system flags of a `FETCH` response into [`MessageFlags`].
#[must_use]
pub(crate) fn flags_from_fetch(fetch: &Fetch) -> MessageFlags {
    let mut flags = MessageFlags::default();
    for flag in fetch.flags() {
        match flag {
            Flag::Seen => flags.seen = true,
            Flag::Answered => flags.answered = true,
            Flag::Flagged => flags.flagged = true,
            Flag::Draft => flags.draft = true,
            Flag::Deleted => flags.deleted = true,
            _ => {}
        }
    }
    flags
}

/// Renders [`MessageFlags`] as an IMAP flag list, e.g. `(\Seen \Flagged)`.
///
/// This is the replace-set form used by `UID STORE ... FLAGS (...)`.
#[must_use]
pub(crate) fn flags_to_imap_list(flags: MessageFlags) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if flags.seen {
        parts.push("\\Seen");
    }
    if flags.answered {
        parts.push("\\Answered");
    }
    if flags.flagged {
        parts.push("\\Flagged");
    }
    if flags.draft {
        parts.push("\\Draft");
    }
    if flags.deleted {
        parts.push("\\Deleted");
    }
    format!("({})", parts.join(" "))
}

/// Builds a [`RemoteEnvelope`] from a `FETCH` response, given the mailbox's
/// current `UIDVALIDITY`.
///
/// Returns `None` when the response lacks a UID, since a stable remote
/// identity cannot be formed without one.
#[must_use]
pub(crate) fn envelope_from_fetch(fetch: &Fetch, uid_validity: u32) -> Option<RemoteEnvelope> {
    let uid = fetch.uid?;
    let flags = flags_from_fetch(fetch);
    let size = fetch.size.map(u64::from);

    let envelope = fetch.envelope();
    let subject = envelope
        .and_then(|env| env.subject.as_deref())
        .map(rfc2047::decode);
    let from = envelope
        .map(|env| convert_addresses(env.from.as_ref()))
        .unwrap_or_default();
    let to = envelope
        .map(|env| convert_addresses(env.to.as_ref()))
        .unwrap_or_default();
    let date = envelope
        .and_then(|env| env.date.as_deref())
        .and_then(date::parse);

    Some(RemoteEnvelope {
        remote_id: RemoteMessageId::Imap { uid, uid_validity },
        subject,
        from,
        to,
        date,
        flags,
        size_bytes: size,
    })
}

/// Converts an optional IMAP address list into core [`Address`] values,
/// dropping entries that are not a usable `mailbox@host` (e.g. group
/// markers or malformed addresses).
fn convert_addresses(addresses: Option<&Vec<ImapAddress>>) -> Vec<Address> {
    let Some(addresses) = addresses else {
        return Vec::new();
    };
    addresses.iter().filter_map(convert_address).collect()
}

/// Converts a single IMAP address, returning `None` when it cannot form a
/// valid email address.
fn convert_address(address: &ImapAddress) -> Option<Address> {
    let mailbox = address.mailbox.as_deref()?;
    let host = address.host.as_deref()?;
    let local = String::from_utf8_lossy(mailbox);
    let domain = String::from_utf8_lossy(host);
    let email = EmailAddress::parse(&format!("{local}@{domain}")).ok()?;
    let display_name = address.name.as_deref().map(rfc2047::decode);
    Some(Address {
        display_name,
        email,
    })
}

/// Formats a sorted, deduplicated slice of UIDs as a compact IMAP set,
/// collapsing consecutive runs into ranges (e.g. `1:3,7,9:11`).
#[must_use]
pub(crate) fn format_uid_set(uids: &[u32]) -> String {
    let mut sorted: Vec<u32> = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut out = String::new();
    let mut idx = 0;
    while idx < sorted.len() {
        let start = sorted[idx];
        let mut end = start;
        while idx + 1 < sorted.len() && sorted[idx + 1] == end + 1 {
            idx += 1;
            end = sorted[idx];
        }
        if !out.is_empty() {
            out.push(',');
        }
        if start == end {
            out.push_str(&start.to_string());
        } else {
            out.push_str(&format!("{start}:{end}"));
        }
        idx += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_list_is_replace_form() {
        let flags = MessageFlags {
            seen: true,
            flagged: true,
            ..MessageFlags::default()
        };
        assert_eq!(flags_to_imap_list(flags), "(\\Seen \\Flagged)");
        assert_eq!(flags_to_imap_list(MessageFlags::default()), "()");
    }

    #[test]
    fn uid_set_collapses_runs() {
        assert_eq!(format_uid_set(&[1, 2, 3, 7, 9, 10, 11]), "1:3,7,9:11");
        assert_eq!(format_uid_set(&[5]), "5");
        assert_eq!(format_uid_set(&[]), "");
        assert_eq!(format_uid_set(&[3, 1, 2, 2]), "1:3");
    }
}
