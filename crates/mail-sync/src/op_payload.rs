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
//! | `move` | `{"sourceMailboxId": 1, "targetMailboxId": 7, "copy": false}` | relocate (or, with `copy`, duplicate) from source to target |
//! | `delete` | `{"sourceMailboxId": 1}` | move to the account's Trash from source |
//!
//! # Legacy payloads without a source
//!
//! `source_mailbox_id` was added in Wave 3 Stage B; pre-Stage-B rows are
//! `{"targetMailboxId": n}` (move) or `{}` (delete). Both fields are therefore
//! `Option`al with a serde default, so an old payload still decodes rather than
//! failing permanently and wedging the account's FIFO queue. When the source is
//! absent the executor falls back to the message's *current* envelope mailbox,
//! reproducing the pre-Stage-B behavior (those ops predate the optimistic local
//! move, so the live mailbox is still the server source).
//!
//! # Why move/copy/delete record their *source* mailbox
//!
//! Flag/keyword ops replay against the message's *live* mailbox (the executor
//! selects `envelope.mailbox`). Move, copy, and delete cannot: the command
//! layer applies the move to local storage **optimistically** (so the UI
//! updates at once), which changes `envelope.mailbox` to the *destination*
//! before the op replays. The server op must still select the **source**
//! mailbox the message physically sits in on the server, so the source id is
//! captured in the payload at enqueue time and the executor selects it, never
//! the (now moved) live mailbox. This is the contract the write-back executor's
//! `apply_one` anticipated for Wave 3 M2.
//!
//! # Why copy reuses the `move` kind
//!
//! Copy is a relocate-to-target op that keeps the source, so it shares the
//! move executor path (select source, then `copy_message` vs `move_message`)
//! and rides the existing `move` [`PendingOpKind`] with a `copy: true`
//! discriminator in the payload. This keeps the change inside the shipped
//! `pending_ops.kind` CHECK domain (`…, 'move', 'delete'`) — no migration, per
//! the Wave 3 ledger (0014 stays unused).

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

/// Payload for [`PendingOpKind::Move`] (a move *or*, with `copy`, a copy): the
/// local ids of the source and target mailboxes, resolved to server names at
/// replay time so a folder rename is tolerated. `source_mailbox_id` is the
/// mailbox the message sits in **on the server**, captured before the
/// optimistic local move changed its live mailbox (see the module docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovePayload {
    /// Server-side source mailbox, captured before the optimistic local move.
    /// `Option` (serde default) so a legacy `{"targetMailboxId": n}` payload
    /// still decodes; a missing source falls back to the live envelope mailbox.
    #[serde(default)]
    source_mailbox_id: Option<MailboxId>,
    target_mailbox_id: MailboxId,
    /// `true` → copy (keep the source); `false` → move. Defaults to a move so
    /// any historical `{source,target}` payload still deserializes.
    #[serde(default)]
    copy: bool,
}

/// Payload for [`PendingOpKind::Delete`] (move-to-Trash): the source mailbox to
/// select on the server (the Trash target is resolved once per drain).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeletePayload {
    /// Server-side source mailbox. `Option` (serde default) so a legacy `{}`
    /// delete payload still decodes; a missing source falls back to the live
    /// envelope mailbox.
    #[serde(default)]
    source_mailbox_id: Option<MailboxId>,
}

/// A resolved relocate op: where the message is, where it goes, and whether the
/// source is kept (copy) or removed (move).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Relocation {
    /// Mailbox to select on the server (the current server location), or `None`
    /// for a legacy payload that recorded no source — the executor then selects
    /// the message's live envelope mailbox.
    pub source: Option<MailboxId>,
    /// Destination mailbox.
    pub target: MailboxId,
    /// `true` for a copy (keep source), `false` for a move.
    pub copy: bool,
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

/// Builds the `(kind, payload)` for setting or clearing `\Answered`.
///
/// Used after a reply is sent to mark the source message answered so the flag
/// propagates to the server (and thus to the sender's other clients).
#[must_use]
pub fn answered_op(answered: bool) -> (PendingOpKind, String) {
    system_flag_op(
        SystemFlags {
            answered: true,
            ..SystemFlags::default()
        },
        answered,
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

/// Builds the `(kind, payload)` for moving a message from `source` to `target`.
#[must_use]
pub fn move_op(source: MailboxId, target: MailboxId) -> (PendingOpKind, String) {
    (
        PendingOpKind::Move,
        encode(&MovePayload {
            source_mailbox_id: Some(source),
            target_mailbox_id: target,
            copy: false,
        }),
    )
}

/// Builds the `(kind, payload)` for copying a message from `source` to
/// `target` (keeping the source). Rides the `move` kind with `copy: true`.
#[must_use]
pub fn copy_op(source: MailboxId, target: MailboxId) -> (PendingOpKind, String) {
    (
        PendingOpKind::Move,
        encode(&MovePayload {
            source_mailbox_id: Some(source),
            target_mailbox_id: target,
            copy: true,
        }),
    )
}

/// Builds the `(kind, payload)` for a delete (move-to-Trash) op from `source`.
#[must_use]
pub fn delete_op(source: MailboxId) -> (PendingOpKind, String) {
    (
        PendingOpKind::Delete,
        encode(&DeletePayload {
            source_mailbox_id: Some(source),
        }),
    )
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

/// Resolves a [`PendingOpKind::Move`] op into its [`Relocation`] (source,
/// target, and whether it is a copy).
pub(crate) fn relocation(op: &PendingOp) -> Result<Relocation, MailError> {
    let payload = decode::<MovePayload>(op)?;
    Ok(Relocation {
        source: payload.source_mailbox_id,
        target: payload.target_mailbox_id,
        copy: payload.copy,
    })
}

/// Resolves the source mailbox to select for a [`PendingOpKind::Delete`] op, or
/// `None` for a legacy payload without one (the executor then uses the live
/// envelope mailbox).
pub(crate) fn delete_source(op: &PendingOp) -> Result<Option<MailboxId>, MailError> {
    Ok(decode::<DeletePayload>(op)?.source_mailbox_id)
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
    fn answered_op_targets_the_answered_system_flag() {
        let (kind, payload) = answered_op(true);
        assert_eq!(kind, PendingOpKind::AddFlags);
        let delta = flag_delta(&op(kind, &payload)).unwrap();
        assert!(delta.add.system.answered);
        assert!(!delta.add.system.seen);
        assert!(delta.remove.is_empty());
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
    fn move_op_round_trips_source_and_target_as_a_move() {
        let (kind, payload) = move_op(MailboxId::new(3), MailboxId::new(42));
        assert_eq!(kind, PendingOpKind::Move);
        let relocation = relocation(&op(kind, &payload)).unwrap();
        assert_eq!(relocation.source, Some(MailboxId::new(3)));
        assert_eq!(relocation.target, MailboxId::new(42));
        assert!(!relocation.copy);
    }

    #[test]
    fn copy_op_rides_the_move_kind_with_the_copy_flag_set() {
        let (kind, payload) = copy_op(MailboxId::new(3), MailboxId::new(42));
        assert_eq!(kind, PendingOpKind::Move);
        let relocation = relocation(&op(kind, &payload)).unwrap();
        assert_eq!(relocation.source, Some(MailboxId::new(3)));
        assert_eq!(relocation.target, MailboxId::new(42));
        assert!(relocation.copy);
    }

    #[test]
    fn delete_op_round_trips_the_source_mailbox() {
        let (kind, payload) = delete_op(MailboxId::new(9));
        assert_eq!(kind, PendingOpKind::Delete);
        assert_eq!(
            delete_source(&op(kind, &payload)).unwrap(),
            Some(MailboxId::new(9))
        );
    }

    #[test]
    fn legacy_move_payload_without_copy_flag_defaults_to_a_move() {
        // A payload predating the `copy` field must still deserialize as a move.
        let relocation = relocation(&op(
            PendingOpKind::Move,
            r#"{"sourceMailboxId":1,"targetMailboxId":2}"#,
        ))
        .unwrap();
        assert!(!relocation.copy);
    }

    #[test]
    fn legacy_move_payload_without_source_decodes_with_no_source() {
        // A pre-Stage-B move op recorded only the target; it must still decode
        // (source `None`) so the executor can fall back to the live mailbox
        // rather than the whole account's queue wedging on a decode failure.
        let relocation = relocation(&op(PendingOpKind::Move, r#"{"targetMailboxId":7}"#)).unwrap();
        assert_eq!(relocation.source, None);
        assert_eq!(relocation.target, MailboxId::new(7));
        assert!(!relocation.copy);
    }

    #[test]
    fn legacy_delete_payload_without_source_decodes_with_no_source() {
        // A pre-Stage-B delete op was the empty object `{}`.
        assert_eq!(
            delete_source(&op(PendingOpKind::Delete, "{}")).unwrap(),
            None
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
