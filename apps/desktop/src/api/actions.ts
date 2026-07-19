/**
 * Typed wrappers over the message-action Tauri commands (move / copy / archive
 * / delete-to-Trash / undo).
 *
 * Move/archive/delete resolve to a {@link MoveOutcome} whose `moves` drive the
 * undo affordance; copy and undo resolve to `null`. All use {@link invokeOrThrow}
 * so a real backend failure rejects (the caller reverts its optimistic list
 * change), while the browser preview degrades to `null` (ADR-0005).
 */
import { invokeOrThrow } from "../lib/tauri";
import type { MessageMove, MoveOutcome } from "../types/mail";

/** Moves a selection of messages into `targetMailboxId`. */
export function moveMessages(
  messageIds: number[],
  targetMailboxId: number,
): Promise<MoveOutcome | null> {
  return invokeOrThrow<MoveOutcome>("move_messages", {
    messageIds,
    targetMailboxId,
  });
}

/** Copies a selection of messages into `targetMailboxId`. */
export function copyMessages(
  messageIds: number[],
  targetMailboxId: number,
): Promise<null> {
  return invokeOrThrow<null>("copy_messages", { messageIds, targetMailboxId });
}

/** Archives a selection of messages (into each account's archive folder). */
export function archiveMessages(
  messageIds: number[],
): Promise<MoveOutcome | null> {
  return invokeOrThrow<MoveOutcome>("archive_messages", { messageIds });
}

/** Deletes a selection of messages to Trash (never a hard delete). */
export function deleteToTrash(messageIds: number[]): Promise<MoveOutcome | null> {
  return invokeOrThrow<MoveOutcome>("delete_to_trash", { messageIds });
}

/** Undoes a prior move/archive/delete by moving each message back. */
export function undoMove(moves: MessageMove[]): Promise<null> {
  return invokeOrThrow<null>("undo_move", { moves });
}
