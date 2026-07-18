//! Stable [`CredentialRef`] construction for accounts.

use axomail_core::{AccountId, CredentialRef};

/// Which server connection of an account a credential authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Incoming mail server (IMAP or POP3).
    Incoming,
    /// Outgoing mail server (SMTP submission).
    Outgoing,
}

impl Direction {
    /// The stable lowercase token used in a credential reference.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// Builds the keychain reference for an account credential.
///
/// The scheme is `account/<id>/<incoming|outgoing>`, for example
/// `account/1/incoming`. It is a **stable on-disk contract**: the same
/// account and direction must always map to the same reference so that a
/// stored secret can be found again across restarts and upgrades. Changing
/// the scheme would orphan every previously stored secret, so it must not
/// change without a migration.
#[must_use]
pub fn credential_ref_for_account(account_id: AccountId, direction: Direction) -> CredentialRef {
    CredentialRef::new(format!(
        "account/{}/{}",
        account_id.value(),
        direction.as_str()
    ))
}
