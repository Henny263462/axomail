//! Value types for the PGP key repository: identifiers, the persisted own-key
//! and recipient-key rows, and their insertion inputs.
//!
//! Only **non-secret** data lives here. An own key carries its armored *public*
//! half and metadata; its armored *secret* half is held in the OS keychain
//! (ADR-0001), keyed by fingerprint, and never appears in these types.

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
    };
}

define_id!(
    /// Identifier of a persisted own-key row.
    PgpOwnKeyId
);
define_id!(
    /// Identifier of a persisted recipient-key row.
    PgpRecipientKeyId
);

/// An OpenPGP keypair we own (public half + metadata; secret in the keychain).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgpOwnKey {
    /// Row identity.
    pub id: PgpOwnKeyId,
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// ASCII-armored public key.
    pub public_armored: String,
    /// Whether the keychain-held secret key is passphrase-protected.
    pub has_passphrase: bool,
    /// Whether this is the default signing key.
    pub is_default: bool,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
    /// When the key was added locally (Unix seconds).
    pub added_at_unix: i64,
}

/// Inputs for inserting an own key. The armored secret is stored separately in
/// the keychain by the caller and is deliberately absent here.
#[derive(Debug, Clone)]
pub struct NewPgpOwnKey {
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// ASCII-armored public key.
    pub public_armored: String,
    /// Whether the secret key is passphrase-protected.
    pub has_passphrase: bool,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
    /// When the key was added locally (Unix seconds).
    pub added_at_unix: i64,
}

/// A recipient / contact public key used to encrypt outgoing mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgpRecipientKey {
    /// Row identity.
    pub id: PgpRecipientKeyId,
    /// Lowercased recipient email this key encrypts to.
    pub email: String,
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// ASCII-armored public key.
    pub public_armored: String,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
    /// When the key was added locally (Unix seconds).
    pub added_at_unix: i64,
}

/// Inputs for inserting (or updating) a recipient key.
#[derive(Debug, Clone)]
pub struct NewPgpRecipientKey {
    /// Lowercased recipient email this key encrypts to.
    pub email: String,
    /// Full fingerprint, lowercase hex.
    pub fingerprint: String,
    /// Long key id, lowercase hex.
    pub key_id: String,
    /// Primary user id string.
    pub user_id: String,
    /// Public-key algorithm of the primary key.
    pub algorithm: String,
    /// ASCII-armored public key.
    pub public_armored: String,
    /// Key creation time (Unix seconds).
    pub created_at_unix: i64,
    /// When the key was added locally (Unix seconds).
    pub added_at_unix: i64,
}
