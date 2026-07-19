import { onUnmounted, ref, type Ref } from "vue";

import {
  markMailboxRead,
  markMessageSeen,
  setMessageFlagged,
} from "../api/messages";
import type {
  AccountSummary,
  MessageListItem,
  UnifiedItemDto,
} from "../types/mail";

/**
 * Dependencies the message-action handlers read and mutate. All are the
 * reactive refs `App.vue` already owns (props-down/events-up; no store).
 */
interface Deps {
  /** Current single-mailbox list (mutated optimistically). */
  messages: Ref<MessageListItem[]>;
  /** Current unified-inbox list (mutated optimistically). */
  unifiedItems: Ref<UnifiedItemDto[]>;
  /** Accounts, whose mailbox unread badges are kept in step. */
  accounts: Ref<AccountSummary[]>;
  /** The open message; a delayed auto-read fires only if it is still open. */
  selectedMessageId: Ref<number | null>;
}

/**
 * Owns the message write-back UX: optimistic star toggling (reverted on a
 * backend error), auto-mark-read-on-display with a configurable delay, and
 * mark-all-read for a mailbox. Every change is applied to the list locally
 * first (the single source of truth the UI reads back) and pushed to the
 * backend, which enqueues the durable server op.
 */
export function useMessageActions(deps: Deps) {
  const autoReadEnabled = ref(true);
  const autoReadDelayMs = ref(0);
  let timer: ReturnType<typeof setTimeout> | null = null;

  /** Applies the persisted auto-mark-read preference. */
  function configureAutoRead(enabled: boolean, delayMs: number): void {
    autoReadEnabled.value = enabled;
    autoReadDelayMs.value = Math.max(0, delayMs);
  }

  function cancelTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  /** Finds a list item by id across the mailbox and unified lists. */
  function findMessage(
    messageId: number,
  ): MessageListItem | UnifiedItemDto | undefined {
    return (
      deps.messages.value.find((m) => m.id === messageId) ??
      deps.unifiedItems.value.find((m) => m.id === messageId)
    );
  }

  /** Keeps the sidebar unread badge in step with an optimistic read. */
  function decrementUnread(mailboxId: number): void {
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) {
        mailbox.unreadCount = Math.max(0, mailbox.unreadCount - 1);
        return;
      }
    }
  }

  /**
   * Schedules the displayed message to be marked read. With auto-read off
   * nothing happens; with a zero delay it marks immediately; otherwise it
   * marks after the delay, and only if the message is still the open one.
   * A newer selection replaces any pending timer.
   */
  function scheduleAutoRead(messageId: number): void {
    cancelTimer();
    const message = findMessage(messageId);
    if (message === undefined || message.seen || !autoReadEnabled.value) return;

    const mark = (): void => {
      if (deps.selectedMessageId.value !== messageId || message.seen) return;
      message.seen = true;
      decrementUnread(message.mailboxId);
      // Browser preview resolves null with nothing to persist; that is fine.
      void markMessageSeen(messageId, true);
    };

    if (autoReadDelayMs.value <= 0) {
      mark();
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      mark();
    }, autoReadDelayMs.value);
  }

  /**
   * Optimistically toggles a message's star and persists it; a backend error
   * reverts the flip so the UI never diverges from the server.
   */
  async function toggleStar(messageId: number): Promise<void> {
    const message = findMessage(messageId);
    if (message === undefined) return;
    const next = !message.flagged;
    message.flagged = next;
    try {
      await setMessageFlagged(messageId, next);
    } catch {
      message.flagged = !next;
    }
  }

  /**
   * Sets a list row's star to `flagged` without a backend call — used to sync
   * the list after the reading pane already persisted the change.
   */
  function syncStar(messageId: number, flagged: boolean): void {
    const message = findMessage(messageId);
    if (message !== undefined) message.flagged = flagged;
  }

  /** Marks every unread message in `mailboxId` read, optimistically. */
  async function markAllRead(mailboxId: number): Promise<void> {
    for (const message of deps.messages.value) {
      if (!message.seen) message.seen = true;
    }
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) mailbox.unreadCount = 0;
    }
    await markMailboxRead(mailboxId);
  }

  onUnmounted(cancelTimer);

  return {
    configureAutoRead,
    scheduleAutoRead,
    toggleStar,
    syncStar,
    markAllRead,
  };
}
