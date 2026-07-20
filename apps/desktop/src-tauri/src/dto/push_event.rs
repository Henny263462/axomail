//! Frontend-facing mirror of push (IMAP IDLE) connection-state changes.
//!
//! Push status is not job-scoped, so it rides its own `axomail://push` channel
//! rather than the job-centric `axomail://sync` stream. The UI uses it to show
//! "connected (push)" versus polling for each account.

use axomail_core::MailError;
use axomail_sync::PushState;
use serde::Serialize;

use super::checked_id;

/// A push-connection status change forwarded to the frontend as
/// `axomail://push`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushStatusDto {
    /// Account whose push state changed.
    pub account_id: i64,
    /// Mailbox the push connection watches when `state` is `"connected"`;
    /// `null` for `"polling"` and `"reconnecting"`.
    pub mailbox_id: Option<i64>,
    /// The new push state: `"connected"`, `"polling"`, or `"reconnecting"`.
    pub state: &'static str,
}

impl PushStatusDto {
    /// Builds the DTO, validating the ids it carries.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the account or mailbox id is out
    /// of the JavaScript safe-integer range.
    pub fn new(
        account_id: i64,
        mailbox_id: Option<i64>,
        state: PushState,
    ) -> Result<Self, MailError> {
        Ok(Self {
            account_id: checked_id(account_id)?,
            mailbox_id: mailbox_id.map(checked_id).transpose()?,
            state: state_str(state),
        })
    }
}

/// Stable camelCase name for a push state value.
fn state_str(state: PushState) -> &'static str {
    match state {
        PushState::Connected => "connected",
        PushState::Polling => "polling",
        PushState::Reconnecting => "reconnecting",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn maps_states_to_stable_names() {
        assert_eq!(
            PushStatusDto::new(1, Some(2), PushState::Connected)
                .unwrap()
                .state,
            "connected"
        );
        assert_eq!(
            PushStatusDto::new(1, None, PushState::Polling)
                .unwrap()
                .state,
            "polling"
        );
        assert_eq!(
            PushStatusDto::new(1, None, PushState::Reconnecting)
                .unwrap()
                .state,
            "reconnecting"
        );
    }

    #[test]
    fn rejects_out_of_range_ids() {
        let too_big = (1_i64 << 53) + 1;
        assert!(PushStatusDto::new(too_big, None, PushState::Polling).is_err());
        assert!(PushStatusDto::new(1, Some(too_big), PushState::Connected).is_err());
    }
}
