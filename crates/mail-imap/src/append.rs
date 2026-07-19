//! Server-side message upload: `APPEND` a raw message into a mailbox.
//!
//! `APPEND` is plain RFC 3501 (no capability gate) and does not require the
//! target mailbox to be selected. After a successful upload the adapter makes a
//! **best-effort** attempt to learn the UID the server assigned — async-imap
//! discards any `APPENDUID` response, so the UID is resolved by selecting the
//! target and searching for the message's `Message-ID`. Failing that resolution
//! never fails the append itself: the message is already stored on the server.

use std::time::Duration;

use axomail_core::{MailError, RemoteMessageId};

use crate::connection::Connected;
use crate::error::{map_imap_error, with_deadline};
use crate::message_ops;

/// Appends `raw` to `encoded_mailbox` (already modified UTF-7 encoded) tagged
/// with `flags` (an IMAP flag list such as `(\Seen)`, or `None` for no flags).
///
/// When `message_id` is given (the bracket-free `Message-ID` of `raw`), the
/// appended message's server UID is resolved best-effort so the caller can
/// adopt the server copy; any resolution failure degrades to `Ok(None)`.
pub(crate) async fn append(
    connected: &mut Connected,
    encoded_mailbox: &str,
    flags: Option<&str>,
    raw: &[u8],
    message_id: Option<&str>,
    timeout: Duration,
) -> Result<Option<RemoteMessageId>, MailError> {
    with_deadline(timeout, async {
        connected
            .session
            .append(encoded_mailbox, flags, None, raw)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;

    // Adoption is a bonus, never a failure mode: the append already committed.
    let Some(message_id) = message_id else {
        return Ok(None);
    };
    Ok(resolve_uid(connected, encoded_mailbox, message_id, timeout)
        .await
        .unwrap_or(None))
}

/// Resolves the appended message's UID by selecting the target and matching its
/// `Message-ID` header (`UID SEARCH HEADER MESSAGE-ID`). The highest matching
/// UID is taken, since the just-appended copy is the newest.
async fn resolve_uid(
    connected: &mut Connected,
    encoded_mailbox: &str,
    message_id: &str,
    timeout: Duration,
) -> Result<Option<RemoteMessageId>, MailError> {
    let uid_validity = message_ops::select(connected, encoded_mailbox, timeout).await?;
    let uids = with_deadline(timeout, async {
        connected
            .session
            .uid_search(format!("HEADER MESSAGE-ID \"{message_id}\""))
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    Ok(uids
        .into_iter()
        .max()
        .map(|uid| RemoteMessageId::Imap { uid, uid_validity }))
}

/// Extracts the bracket-free `Message-ID` from the header block of a raw RFC
/// 5322 message, or `None` when the message declares none.
///
/// Only the header block (up to the first blank line) is scanned. A single
/// level of header folding (continuation lines beginning with whitespace) is
/// tolerated, which covers the way message builders emit long `Message-ID`s.
pub(crate) fn parse_message_id(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let mut lines = text.lines();
    let mut value = String::new();
    let mut collecting = false;
    for line in lines.by_ref() {
        if line.is_empty() {
            break; // End of the header block.
        }
        if collecting {
            if line.starts_with([' ', '\t']) {
                value.push_str(line.trim());
                continue;
            }
            break;
        }
        if let Some(rest) = strip_message_id_prefix(line) {
            value.push_str(rest.trim());
            collecting = true;
        }
    }
    if !collecting {
        return None;
    }
    let start = value.find('<')?;
    let end = value[start..].find('>')? + start;
    let inner = value[start + 1..end].trim();
    (!inner.is_empty()).then(|| inner.to_owned())
}

/// Returns the text after a case-insensitive `Message-ID:` header name, or
/// `None` when `line` is not that header.
fn strip_message_id_prefix(line: &str) -> Option<&str> {
    let colon = line.find(':')?;
    let (name, rest) = line.split_at(colon);
    name.trim()
        .eq_ignore_ascii_case("message-id")
        .then_some(&rest[1..])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parses_a_simple_message_id() {
        let raw = b"From: a@example.org\r\nMessage-ID: <abc.123@example.org>\r\n\r\nbody";
        assert_eq!(
            parse_message_id(raw).as_deref(),
            Some("abc.123@example.org")
        );
    }

    #[test]
    fn is_case_insensitive_and_stops_at_body() {
        let raw = b"Subject: hi\r\nmessage-id:  <x@y>  \r\n\r\nMessage-ID: <not-a-header@z>";
        assert_eq!(parse_message_id(raw).as_deref(), Some("x@y"));
    }

    #[test]
    fn tolerates_a_folded_header() {
        let raw = b"Message-ID:\r\n <folded.id@example.org>\r\n\r\nbody";
        assert_eq!(
            parse_message_id(raw).as_deref(),
            Some("folded.id@example.org")
        );
    }

    #[test]
    fn returns_none_without_a_message_id() {
        let raw = b"From: a@example.org\r\nSubject: hi\r\n\r\nbody";
        assert_eq!(parse_message_id(raw), None);
    }
}
