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
///
/// Password accounts store a distinct secret per direction here. OAuth2
/// accounts instead share one token set across both directions under
/// [`credential_ref_for_oauth`].
#[must_use]
pub fn credential_ref_for_account(account_id: AccountId, direction: Direction) -> CredentialRef {
    CredentialRef::new(format!(
        "account/{}/{}",
        account_id.value(),
        direction.as_str()
    ))
}

/// Builds the account-scoped keychain reference for an OAuth2 token set.
///
/// The scheme is `account/<id>/oauth`, for example `account/1/oauth`. Unlike
/// password credentials, an OAuth2 account has **one** token set (access +
/// refresh) that serves both the incoming and outgoing connections, so both
/// directions resolve from — and every refresh writes back to — this single
/// reference. Storing per-direction copies would let a rotating refresh token
/// (Microsoft) invalidate the other direction's stored copy.
///
/// Like [`credential_ref_for_account`], this is a stable on-disk contract.
#[must_use]
pub fn credential_ref_for_oauth(account_id: AccountId) -> CredentialRef {
    CredentialRef::new(format!("account/{}/oauth", account_id.value()))
}
