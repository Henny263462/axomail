//! JSON encoding of composer recipient lists.
//!
//! Drafts and outbox rows keep their `to`/`cc`/`bcc` recipients as JSON
//! text columns (see migration `0003_drafts_and_outbox.sql`) so the
//! composer can round-trip them without a recipient join. The composer
//! contract carries recipients as plain address strings, so each column is
//! a JSON array of strings; richer address objects are a later enhancement
//! and would be a forward-compatible change to the array element shape.

use axomail_core::MailError;

/// Serializes a recipient list into the JSON text stored in a column.
///
/// Serialization of a `Vec<String>` cannot fail in practice; a failure is
/// still mapped to [`MailError::Database`] rather than panicking.
pub(crate) fn encode_addresses(addresses: &[String]) -> Result<String, MailError> {
    serde_json::to_string(addresses).map_err(|_| MailError::Database {
        message: "encode_addresses: could not serialize recipient list".to_owned(),
    })
}

/// Parses the JSON text of a recipient column back into a list.
///
/// # Errors
///
/// Returns [`MailError::Database`] if the stored text is not a JSON array
/// of strings.
pub(crate) fn decode_addresses(json: &str) -> Result<Vec<String>, MailError> {
    serde_json::from_str(json).map_err(|_| MailError::Database {
        message: "decode_addresses: stored recipient list is not a JSON string array".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn round_trips_a_recipient_list() {
        let input = vec!["a@example.org".to_owned(), "b@example.org".to_owned()];
        let json = encode_addresses(&input).unwrap();
        assert_eq!(decode_addresses(&json).unwrap(), input);
    }

    #[test]
    fn empty_list_round_trips() {
        let json = encode_addresses(&[]).unwrap();
        assert_eq!(json, "[]");
        assert!(decode_addresses(&json).unwrap().is_empty());
    }

    #[test]
    fn rejects_non_array_json() {
        assert!(decode_addresses("not json").is_err());
        assert!(decode_addresses("{\"a\":1}").is_err());
    }
}
