/**
 * Typed wrappers over the message Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { MessageDetailDto, MessageListPage } from "../types/mail";

/**
 * Loads one page of a mailbox's messages, newest first.
 *
 * @param mailboxId Mailbox to list.
 * @param cursor Opaque cursor from a previous page, or `null` for the first.
 * @param limit Maximum number of envelopes to return.
 */
export function listMessages(
  mailboxId: number,
  cursor: string | null,
  limit: number,
): Promise<MessageListPage | null> {
  return safeInvoke<MessageListPage>("list_messages", {
    mailboxId,
    cursor,
    limit,
  });
}

/** Sets (or clears) the seen flag on a single message. */
export function markMessageSeen(
  messageId: number,
  seen: boolean,
): Promise<null> {
  return safeInvoke<null>("mark_message_seen", { messageId, seen });
}

/**
 * Sets (or clears) the star (flagged) flag on a single message.
 *
 * Uses {@link invokeOrThrow} so a backend failure rejects — the caller
 * reverts its optimistic flip on rejection. Resolves `null` in the browser
 * preview, where the optimistic flip simply stands.
 */
export function setMessageFlagged(
  messageId: number,
  flagged: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_message_flagged", { messageId, flagged });
}

/**
 * Marks every unread message in a mailbox read. Resolves the number of
 * messages marked, or `null` in the browser preview.
 */
export function markMailboxRead(mailboxId: number): Promise<number | null> {
  return safeInvoke<number>("mark_mailbox_read", { mailboxId });
}

/**
 * Sets the read state of a specific set of messages (the tag-filter view's
 * "Mark all read", whose messages can span mailboxes).
 *
 * The command returns nothing, so success and failure cannot be told apart by
 * a `null` result; {@link invokeOrThrow} is used so a real backend failure
 * rejects (the caller reverts its optimistic update on rejection). The browser
 * preview still resolves `null`, leaving the optimistic update to stand.
 */
export function markMessagesRead(
  messageIds: number[],
  seen: boolean,
): Promise<null> {
  return invokeOrThrow<null>("mark_messages_read", { messageIds, seen });
}

/**
 * Loads the full detail of one message, including the sanitized viewing
 * payload. Resolves to `null` in the browser preview or on backend failure.
 *
 * `allowRemote` is the allow-once override: `true` shows remote images for
 * this load regardless of the stored per-sender decision; omit it (or pass
 * `null`) to defer to that decision.
 */
export function getMessageDetail(
  messageId: number,
  allowRemote: boolean | null = null,
): Promise<MessageDetailDto | null> {
  return safeInvoke<MessageDetailDto>("get_message_detail", {
    messageId,
    allowRemote,
  });
}

/**
 * Persists (or clears) an "always show remote content" decision for a sender.
 * Resolves to `null` in the browser preview or on backend failure.
 */
export function setRemoteContentAllowed(
  sender: string,
  allowed: boolean,
): Promise<null> {
  return safeInvoke<null>("set_remote_content_allowed", { sender, allowed });
}
