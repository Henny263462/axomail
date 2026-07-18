//! Typed identifiers for core entities.
//!
//! Local IDs wrap the database key of the corresponding row. Remote IDs
//! wrap protocol-level identifiers in a protocol-agnostic form so that
//! storage and sync never handle raw protocol values.

use serde::{Deserialize, Serialize};

macro_rules! define_local_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(i64);

        impl $name {
            /// Wraps a raw database key.
            #[must_use]
            pub const fn new(value: i64) -> Self {
                Self(value)
            }

            /// Returns the raw database key.
            #[must_use]
            pub const fn value(self) -> i64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_local_id!(
    /// Identifier of a configured account.
    AccountId
);
define_local_id!(
    /// Identifier of a mailbox (folder) within an account.
    MailboxId
);
define_local_id!(
    /// Identifier of a locally stored message.
    MessageId
);
define_local_id!(
    /// Identifier of a locally stored attachment.
    AttachmentId
);

/// Non-secret reference to a credential stored in the operating-system
/// keychain.
///
/// The referenced secret itself never appears in the database, in logs,
/// or in any core type; resolving the reference is the exclusive job of
/// `mail-security`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialRef(String);

impl CredentialRef {
    /// Wraps a keychain entry reference.
    #[must_use]
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    /// Returns the keychain entry reference.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Protocol-agnostic identifier of a message on a remote server.
///
/// The variants mirror the identity semantics of the supported incoming
/// protocols. Only adapters construct these; storage and sync treat them
/// as opaque values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemoteMessageId {
    /// IMAP identity: a UID that is only meaningful together with the
    /// mailbox's `UIDVALIDITY` at the time it was issued.
    Imap {
        /// Message UID within the mailbox.
        uid: u32,
        /// `UIDVALIDITY` of the mailbox when the UID was recorded.
        uid_validity: u32,
    },
    /// POP3 identity: the server-assigned `UIDL` string.
    Pop3 {
        /// Unique-id listing value for the message.
        uidl: String,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn local_ids_round_trip_and_are_distinct_types() {
        let account = AccountId::new(7);
        assert_eq!(account.value(), 7);
        assert_eq!(account, AccountId::new(7));
        assert_eq!(account.to_string(), "7");
    }

    #[test]
    fn local_ids_serialize_transparently() {
        let json = serde_json::to_string(&MessageId::new(42)).unwrap();
        assert_eq!(json, "42");
        let back: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MessageId::new(42));
    }

    #[test]
    fn remote_ids_distinguish_protocols() {
        let imap = RemoteMessageId::Imap {
            uid: 5,
            uid_validity: 99,
        };
        let pop3 = RemoteMessageId::Pop3 {
            uidl: "abc123".into(),
        };
        assert_ne!(imap, pop3);
    }
}
