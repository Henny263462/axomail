import { onUnmounted, ref, type Ref } from "vue";

import {
  markMailboxRead,
  markMessagesRead,
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
  /** Current single-mailbox / tag-filter list (mutated optimistically). */
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
 * mark-all-read for a mailbox or a message set. Every change is applied to the
 * list locally first (the single source of truth the UI reads back) and pushed
 * to the backend, which enqueues the durable server op.
 */
export function useMessageActions(deps: Deps) {
  const autoReadEnabled = ref(true);
  const autoReadDelayMs = ref(0);
  // Auto-read stays inert until the persisted setting has been applied at
  // least once, so a message opened during the initial settings load is never
  // marked read against a persisted preference that disables it.
  const autoReadConfigured = ref(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  /** Applies the persisted auto-mark-read preference (idempotent, reactive). */
  function configureAutoRead(enabled: boolean, delayMs: number): void {
    autoReadEnabled.value = enabled;
    autoReadDelayMs.value = Math.max(0, delayMs);
    autoReadConfigured.value = true;
  }

  function cancelTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  /** Finds a list item by id across the mailbox/tag and unified lists. */
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
   * Schedules the displayed message to be marked read. With auto-read off (or
   * not yet configured) nothing happens; with a zero delay it marks
   * immediately; otherwise it marks after the delay, and only if the message
   * is still the open one. A newer selection replaces any pending timer.
   */
  function scheduleAutoRead(messageId: number): void {
    cancelTimer();
    if (!autoReadConfigured.value || !autoReadEnabled.value) return;
    const message = findMessage(messageId);
    if (message === undefined || message.seen) return;

    // Re-resolve the row at fire time: the list may have been reloaded (a new
    // page of objects) during the delay, so the captured object could be
    // detached. Marking must target the still-current row for the still-open
    // message, never a stale one.
    const mark = (): void => {
      if (deps.selectedMessageId.value !== messageId) return;
      const current = findMessage(messageId);
      if (current === undefined || current.seen) return;
      current.seen = true;
      decrementUnread(current.mailboxId);
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

  /**
   * Toggles a single message's read/unread state optimistically (the `u`
   * shortcut / command). The list row and the sidebar unread badge flip at
   * once, then the change is persisted; a backend failure reverts both. Uses
   * {@link markMessagesRead} (which rejects on failure) so the revert is exact,
   * mirroring {@link markListRead}; the browser preview resolves with nothing to
   * persist and the optimistic flip stands.
   */
  async function toggleSeen(messageId: number): Promise<void> {
    const message = findMessage(messageId);
    if (message === undefined) return;
    const next = !message.seen;
    message.seen = next;
    if (next) decrementUnread(message.mailboxId);
    else incrementUnread(message.mailboxId);
    try {
      await markMessagesRead([messageId], next);
    } catch {
      message.seen = !next;
      if (next) incrementUnread(message.mailboxId);
      else decrementUnread(message.mailboxId);
    }
  }

  /**
   * Marks every unread message in `mailboxId` read, optimistically, reverting
   * the whole batch if the command fails (a `null` result — which
   * {@link markMailboxRead} returns on failure — or a throw).
   */
  async function markAllRead(mailboxId: number): Promise<void> {
    const flipped = flipUnreadRows();
    let badge: { mailbox: AccountSummary["mailboxes"][number]; previous: number } | null =
      null;
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) {
        badge = { mailbox, previous: mailbox.unreadCount };
        mailbox.unreadCount = 0;
        break;
      }
    }
    // `markMailboxRead` returns the marked count on success and `null` on
    // failure (safeInvoke swallows the error), so a `null` means undo.
    const result = await markMailboxRead(mailboxId);
    if (result === null) {
      revertUnreadRows(flipped);
      if (badge !== null) badge.mailbox.unreadCount = badge.previous;
    }
  }

  /**
   * Marks every unread message in the current list read (the tag-filter view,
   * whose messages can span mailboxes). Optimistic, with the same batch revert
   * on failure as {@link markAllRead}.
   */
  async function markListRead(): Promise<void> {
    const flipped = flipUnreadRows();
    if (flipped.length === 0) return;
    for (const message of flipped) decrementUnread(message.mailboxId);
    try {
      await markMessagesRead(
        flipped.map((message) => message.id),
        true,
      );
    } catch {
      // A real backend failure: undo the whole optimistic batch.
      revertUnreadRows(flipped);
      for (const message of flipped) incrementUnread(message.mailboxId);
    }
  }

  /** Flips every unread row in the current list to seen, returning them. */
  function flipUnreadRows(): MessageListItem[] {
    const flipped: MessageListItem[] = [];
    for (const message of deps.messages.value) {
      if (!message.seen) {
        message.seen = true;
        flipped.push(message);
      }
    }
    return flipped;
  }

  /** Restores rows flipped by {@link flipUnreadRows} to unread. */
  function revertUnreadRows(rows: MessageListItem[]): void {
    for (const message of rows) message.seen = false;
  }

  /** Restores an optimistic unread-badge decrement. */
  function incrementUnread(mailboxId: number): void {
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) {
        mailbox.unreadCount += 1;
        return;
      }
    }
  }

  onUnmounted(cancelTimer);

  return {
    configureAutoRead,
    scheduleAutoRead,
    toggleStar,
    toggleSeen,
    syncStar,
    markAllRead,
    markListRead,
  };
}
