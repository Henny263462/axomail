/**
 * Typed wrappers over the message Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
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
