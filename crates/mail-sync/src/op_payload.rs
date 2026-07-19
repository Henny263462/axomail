//! Payloads for the offline operation queue (`pending_ops`).
//!
//! Storage keeps a pending op's payload as opaque JSON text (validated only as
//! well-formed JSON on enqueue). This module owns the *meaning* of that text:
//! the command layer uses the builders here to construct a `(kind, payload)`
//! pair for [`axomail_storage::NewPendingOp`], and the write-back executor uses
//! the interpreters to turn a claimed op back into a [`FlagDelta`] or a move
//! target. Keeping the schema in one place stops the enqueue and replay sides
//! from drifting.
//!
//! The per-kind payload shapes are:
//!
//! | kind | payload | meaning |
//! |------|---------|---------|
//! | `add_flags` / `remove_flags` | `{"system": {..}}` | system flags to `+FLAGS` / `-FLAGS` |
//! | `add_keyword` / `remove_keyword` | `{"keyword": "$Junk"}` | one IMAP keyword to add / remove |
//! | `move` | `{"targetMailboxId": 7}` | local id of the destination mailbox |
//! | `delete` | `{}` | move to the account's Trash (no extra data) |

use axomail_core::{FlagDelta, FlagSet, Keyword, MailError, MailboxId, SystemFlags};
use axomail_storage::{PendingOp, PendingOpKind};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Payload for [`PendingOpKind::AddFlags`] / [`PendingOpKind::RemoveFlags`]:
/// the system flags to add or remove (direction is the kind).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlagsPayload {
    system: SystemFlags,
}

/// Payload for [`PendingOpKind::AddKeyword`] / [`PendingOpKind::RemoveKeyword`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeywordPayload {
    keyword: Keyword,
}

/// Payload for [`PendingOpKind::Move`]: the local id of the target mailbox,
/// resolved to a server name at replay time so a folder rename is tolerated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovePayload {
    target_mailbox_id: MailboxId,
}

/// Builds the `(kind, payload)` for setting or clearing `\Seen` on a message.
///
/// `seen = true` adds the flag (`+FLAGS \Seen`); `seen = false` removes it.
#[must_use]
pub fn seen_op(seen: bool) -> (PendingOpKind, String) {
    system_flag_op(
        SystemFlags {
            seen: true,
            ..SystemFlags::default()
        },
        seen,
    )
}

/// Builds the `(kind, payload)` for setting or clearing `\Flagged` (star).
#[must_use]
pub fn flagged_op(flagged: bool) -> (PendingOpKind, String) {
    system_flag_op(
        SystemFlags {
            flagged: true,
            ..SystemFlags::default()
        },
        flagged,
    )
}

/// Builds the `(kind, payload)` for adding or removing one IMAP keyword (tag)
/// on a message. `add = true` issues `+FLAGS (keyword)`; `add = false` issues
/// `-FLAGS (keyword)`. Used by the tag/untag command to enqueue a keyword op
/// that the single write-back executor drains additively.
#[must_use]
pub fn keyword_op(keyword: Keyword, add: bool) -> (PendingOpKind, String) {
    let kind = if add {
        PendingOpKind::AddKeyword
    } else {
        PendingOpKind::RemoveKeyword
    };
    (kind, encode(&KeywordPayload { keyword }))
}

/// Builds the `(kind, payload)` for moving a message to `target`.
#[must_use]
pub fn move_op(target: MailboxId) -> (PendingOpKind, String) {
    (
        PendingOpKind::Move,
        encode(&MovePayload {
            target_mailbox_id: target,
        }),
    )
}

/// Builds the `(kind, payload)` for a delete (move-to-Trash) op.
#[must_use]
pub fn delete_op() -> (PendingOpKind, String) {
    (PendingOpKind::Delete, "{}".to_owned())
}

/// Common builder for a system-flag op: `add` chooses `+FLAGS` vs `-FLAGS`.
fn system_flag_op(system: SystemFlags, add: bool) -> (PendingOpKind, String) {
    let kind = if add {
        PendingOpKind::AddFlags
    } else {
        PendingOpKind::RemoveFlags
    };
    (kind, encode(&FlagsPayload { system }))
}

/// Serializes a payload to JSON text; a failure (never expected for these
/// plain structs) degrades to the empty object rather than panicking.
fn encode<T: Serialize>(payload: &T) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_owned())
}

/// Interprets a flag/keyword op as the [`FlagDelta`] the adapter should
/// `store_flags`. Returns [`MailError::Configuration`] for a non-flag kind or
/// a malformed payload (a poison op the executor parks, never a wedge).
pub(crate) fn flag_delta(op: &PendingOp) -> Result<FlagDelta, MailError> {
    match op.kind {
        PendingOpKind::AddFlags => Ok(FlagDelta {
            add: FlagSet::from_system(decode::<FlagsPayload>(op)?.system),
            remove: FlagSet::default(),
        }),
        PendingOpKind::RemoveFlags => Ok(FlagDelta {
            add: FlagSet::default(),
            remove: FlagSet::from_system(decode::<FlagsPayload>(op)?.system),
        }),
        PendingOpKind::AddKeyword => Ok(FlagDelta {
            add: FlagSet::from_keyword(decode::<KeywordPayload>(op)?.keyword),
            remove: FlagSet::default(),
        }),
        PendingOpKind::RemoveKeyword => Ok(FlagDelta {
            add: FlagSet::default(),
            remove: FlagSet::from_keyword(decode::<KeywordPayload>(op)?.keyword),
        }),
        PendingOpKind::Move | PendingOpKind::Delete => Err(MailError::Configuration {
            message: "pending op is not a flag change".to_owned(),
        }),
    }
}

/// Resolves the destination mailbox id of a [`PendingOpKind::Move`] op.
pub(crate) fn move_target(op: &PendingOp) -> Result<MailboxId, MailError> {
    Ok(decode::<MovePayload>(op)?.target_mailbox_id)
}

/// Deserializes an op's payload, mapping a decode failure to a user-safe
/// configuration error (parked, so a malformed op never wedges its account).
fn decode<T: DeserializeOwned>(op: &PendingOp) -> Result<T, MailError> {
    serde_json::from_str(&op.payload_json).map_err(|_| MailError::Configuration {
        message: format!("pending op {} carries a malformed payload", op.id),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use axomail_core::{AccountId, MessageId};
    use axomail_storage::{PendingOpId, PendingOpState};

    fn op(kind: PendingOpKind, payload: &str) -> PendingOp {
        PendingOp {
            id: PendingOpId::new(1),
            account_id: AccountId::new(1),
            message_id: MessageId::new(1),
            kind,
            payload_json: payload.to_owned(),
            state: PendingOpState::Pending,
            attempts: 0,
            last_error: None,
            created_at_unix: 0,
            updated_at_unix: 0,
        }
    }

    #[test]
    fn seen_op_round_trips_to_an_additive_seen_delta() {
        let (kind, payload) = seen_op(true);
        assert_eq!(kind, PendingOpKind::AddFlags);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.add.system.seen);
        assert!(delta.remove.is_empty());
    }

    #[test]
    fn clearing_seen_builds_a_remove_delta() {
        let (kind, payload) = seen_op(false);
        assert_eq!(kind, PendingOpKind::RemoveFlags);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.remove.system.seen);
        assert!(delta.add.is_empty());
    }

    #[test]
    fn flagged_op_targets_the_flagged_system_flag() {
        let (kind, payload) = flagged_op(true);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.add.system.flagged);
        assert!(!delta.add.system.seen);
    }

    #[test]
    fn keyword_op_round_trips_to_an_additive_keyword_delta() {
        let keyword = Keyword::parse("$Junk").unwrap();
        let (kind, payload) = keyword_op(keyword.clone(), true);
        assert_eq!(kind, PendingOpKind::AddKeyword);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.add.keywords.contains(&keyword));
        assert!(delta.remove.is_empty());

        let (kind, payload) = keyword_op(keyword.clone(), false);
        assert_eq!(kind, PendingOpKind::RemoveKeyword);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.remove.keywords.contains(&keyword));
        assert!(delta.add.is_empty());
    }

    #[test]
    fn move_op_round_trips_the_target_mailbox() {
        let (kind, payload) = move_op(MailboxId::new(42));
        assert_eq!(kind, PendingOpKind::Move);
        assert_eq!(
            move_target(&op(kind, &payload)).unwrap(),
            MailboxId::new(42)
        );
    }

    #[test]
    fn malformed_payload_is_a_configuration_error_not_a_panic() {
        let bad = op(PendingOpKind::AddFlags, "not json");
        assert!(matches!(
            flag_delta(&bad),
            Err(MailError::Configuration { .. })
        ));
    }

    #[test]
    fn flag_delta_rejects_move_and_delete_kinds() {
        assert!(flag_delta(&op(PendingOpKind::Move, "{}")).is_err());
        assert!(flag_delta(&op(PendingOpKind::Delete, "{}")).is_err());
    }
}
