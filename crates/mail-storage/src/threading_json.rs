//! Column mapping and JSON encoding for a message's threading identity.
//!
//! Migration 0006 adds three nullable columns to `messages`:
//! `message_id`, `in_reply_to` (the first parent id), and `references_json`
//! (a JSON array of message-ids, following the [`crate::address_json`]
//! precedent). `references_json` doubles as the "threading extracted"
//! sentinel: `NULL` marks a row whose headers have not been scanned yet,
//! while any JSON array — including `"[]"` — marks an extracted row, so the
//! lazy backfill on open runs exactly once per message.

use axomail_core::{MailError, ThreadingHeaders};

/// The three threading columns of a `messages` row, ready to bind.
pub(crate) struct ThreadingColumns {
    /// `message_id` column: the message's own id, or `None`.
    pub(crate) message_id: Option<String>,
    /// `in_reply_to` column: the first In-Reply-To id, or `None`.
    pub(crate) in_reply_to: Option<String>,
    /// `references_json` column: `None` marks "not yet extracted";
    /// `Some(json)` (including `"[]"`) marks an extracted row.
    pub(crate) references_json: Option<String>,
}

impl ThreadingColumns {
    /// Columns for a row whose threading identity has not been extracted
    /// yet (all `NULL`), so the backfill-on-open path will populate it.
    pub(crate) fn unextracted() -> Self {
        Self {
            message_id: None,
            in_reply_to: None,
            references_json: None,
        }
    }

    /// Columns derived from extracted threading headers. `references_json`
    /// is always `Some` (an empty chain becomes `"[]"`), marking the row
    /// extracted even when it carries no ids.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the references list cannot be
    /// serialized (not expected for a `Vec<String>`).
    pub(crate) fn from_headers(headers: &ThreadingHeaders) -> Result<Self, MailError> {
        Ok(Self {
            message_id: headers.message_id.clone(),
            in_reply_to: headers.in_reply_to.first().cloned(),
            references_json: Some(encode_references(&headers.references)?),
        })
    }
}

/// Serializes a References chain into the JSON text stored in a column.
///
/// Serialization of a `Vec<String>` cannot fail in practice; a failure is
/// still mapped to [`MailError::Database`] rather than panicking.
pub(crate) fn encode_references(references: &[String]) -> Result<String, MailError> {
    serde_json::to_string(references).map_err(|_| MailError::Database {
        message: "encode_references: could not serialize the references chain".to_owned(),
    })
}

/// Parses the JSON text of a `references_json` column back into a list.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the stored text is not a JSON array
/// of strings.
pub(crate) fn decode_references(json: &str) -> Result<Vec<String>, MailError> {
    serde_json::from_str(json).map_err(|_| MailError::Database {
        message: "decode_references: stored references chain is not a JSON string array".to_owned(),
    })
}

/// Rebuilds [`ThreadingHeaders`] from the three stored columns.
///
/// A `NULL` `references_json` (never-extracted row) decodes to an empty
/// chain, so the reconstructed headers are simply empty.
///
/// # Errors
///
/// Returns [`MailError::Database`] if `references_json` holds text that is
/// not a JSON array of strings.
pub(crate) fn decode_threading(
    message_id: Option<String>,
    in_reply_to: Option<String>,
    references_json: Option<&str>,
) -> Result<ThreadingHeaders, MailError> {
    Ok(ThreadingHeaders {
        message_id,
        in_reply_to: in_reply_to.into_iter().collect(),
        references: match references_json {
            Some(json) => decode_references(json)?,
            None => Vec::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn from_headers_marks_extracted_even_when_empty() {
        let columns = ThreadingColumns::from_headers(&ThreadingHeaders::default()).unwrap();
        assert!(columns.message_id.is_none());
        assert!(columns.in_reply_to.is_none());
        // An empty chain still marks the row extracted.
        assert_eq!(columns.references_json.as_deref(), Some("[]"));
    }

    #[test]
    fn from_headers_keeps_first_in_reply_to_and_full_references() {
        let headers = ThreadingHeaders {
            message_id: Some("self@x".to_owned()),
            in_reply_to: vec!["p1@x".to_owned(), "p2@x".to_owned()],
            references: vec!["r0@x".to_owned(), "r1@x".to_owned()],
        };
        let columns = ThreadingColumns::from_headers(&headers).unwrap();
        assert_eq!(columns.message_id.as_deref(), Some("self@x"));
        assert_eq!(columns.in_reply_to.as_deref(), Some("p1@x"));
        assert_eq!(
            columns.references_json.as_deref(),
            Some("[\"r0@x\",\"r1@x\"]")
        );
    }

    #[test]
    fn unextracted_columns_decode_to_empty_headers() {
        let columns = ThreadingColumns::unextracted();
        let headers = decode_threading(
            columns.message_id,
            columns.in_reply_to,
            columns.references_json.as_deref(),
        )
        .unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn decode_threading_round_trips_columns() {
        let headers = decode_threading(
            Some("self@x".to_owned()),
            Some("p1@x".to_owned()),
            Some("[\"r0@x\",\"r1@x\"]"),
        )
        .unwrap();
        assert_eq!(headers.message_id.as_deref(), Some("self@x"));
        assert_eq!(headers.in_reply_to, vec!["p1@x".to_owned()]);
        assert_eq!(
            headers.references,
            vec!["r0@x".to_owned(), "r1@x".to_owned()]
        );
    }

    #[test]
    fn decode_rejects_non_array_json() {
        assert!(decode_references("not json").is_err());
        assert!(decode_references("{\"a\":1}").is_err());
    }
}
