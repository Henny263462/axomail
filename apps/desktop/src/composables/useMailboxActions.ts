import { onScopeDispose, ref, type Ref } from "vue";

import {
  archiveMessages,
  copyMessages,
  deleteToTrash,
  moveMessages,
  undoMove,
} from "../api/actions";
import { markMessagesRead } from "../api/messages";
import { isTauri } from "../lib/tauri";
import type { DialogController } from "./useDialogs";
import type { AppDialogMap } from "../components/layout/dialogs";
import type {
  AccountSummary,
  MailboxSummary,
  MessageListItem,
  MessageMove,
  UnifiedItemDto,
} from "../types/mail";

/** How long the undo toast stays before auto-dismissing. */
const UNDO_TIMEOUT_MS = 8000;
/** How long a transient notice (copy/error) stays. */
const NOTICE_TIMEOUT_MS = 4000;
/** Shown when a mutating action cannot reach the backend (browser preview). */
const UNAVAILABLE = "This action needs the desktop app.";

/** The bulk actions the list/context-menu surfaces dispatch. */
export type BulkAction = "archive" | "delete" | "move" | "copy" | "markRead";

/** A folder chosen in the move/copy picker, with the request it answers. */
export interface FolderPicked {
  ids: number[];
  mode: "move" | "copy";
  mailboxId: number;
  mailboxName: string;
}

interface Deps {
  /** Single-mailbox / tag list (rows are spliced out on a relocation). */
  messages: Ref<MessageListItem[]>;
  /** Unified-inbox list (also spliced). */
  unifiedItems: Ref<UnifiedItemDto[]>;
  /** Accounts, whose source-folder unread badge is kept in step. */
  accounts: Ref<AccountSummary[]>;
  /** The open message; cleared when it is relocated out of view. */
  selectedMessageId: Ref<number | null>;
  /** App dialog controller, used to open the move/copy folder picker. */
  dialogs: DialogController<AppDialogMap>;
  /** Reloads the current list from the backend (after an undo). */
  reload: () => void;
  /** Clears the list's multi-selection after a completed bulk relocation. */
  clearSelection?: () => void;
}

/** A snapshot for reverting an optimistic relocation exactly. */
interface Relocation {
  prevMessages: MessageListItem[];
  prevUnified: UnifiedItemDto[];
  /** Mailboxes whose unread badge was decremented, with their prior value. */
  badgeReverts: { mailbox: MailboxSummary; previous: number }[];
  /** The open message id cleared by this relocation, or null. */
  clearedSelection: number | null;
}

/**
 * Owns the reading-pane / list message actions (move, copy, archive,
 * delete-to-Trash) and their undo. Each relocation is applied optimistically —
 * the affected rows are spliced out of the current list at once — then pushed
 * to the backend; a failure (or an unreachable backend) reverts by restoring
 * the exact snapshot, and a success raises a time-boxed undo toast whose
 * {@link MessageMove} records move the messages back. Copy leaves the source in
 * place and is not undoable.
 */
export function useMailboxActions(deps: Deps) {
  const undoState = ref<{ message: string; moves: MessageMove[] } | null>(null);
  const notice = ref<string | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | null = null;
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  function showUndo(message: string, moves: MessageMove[]): void {
    if (moves.length === 0) return;
    if (undoTimer !== null) clearTimeout(undoTimer);
    undoState.value = { message, moves };
    undoTimer = setTimeout(() => {
      undoState.value = null;
      undoTimer = null;
    }, UNDO_TIMEOUT_MS);
  }

  function dismissUndo(): void {
    if (undoTimer !== null) clearTimeout(undoTimer);
    undoTimer = null;
    undoState.value = null;
  }

  function showNotice(message: string): void {
    if (noticeTimer !== null) clearTimeout(noticeTimer);
    notice.value = message;
    noticeTimer = setTimeout(() => {
      notice.value = null;
      noticeTimer = null;
    }, NOTICE_TIMEOUT_MS);
  }

  /** Finds the mailbox with `id` across all accounts. */
  function findMailbox(mailboxId: number): MailboxSummary | undefined {
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) return mailbox;
    }
    return undefined;
  }

  /** Decrements the source folder's unread badge by one (clamped at zero). */
  function decrementUnread(mailboxId: number): void {
    const mailbox = findMailbox(mailboxId);
    if (mailbox !== undefined) {
      mailbox.unreadCount = Math.max(0, mailbox.unreadCount - 1);
    }
  }

  /**
   * Splices `ids` out of both caches and returns a snapshot {@link restore} can
   * undo exactly. A message present in both the mailbox and unified caches is
   * counted once, so its source-folder unread badge drops by exactly one.
   */
  function removeRows(ids: number[]): Relocation {
    const set = new Set(ids);
    const prevMessages = deps.messages.value;
    const prevUnified = deps.unifiedItems.value;
    const prevSelected = deps.selectedMessageId.value;

    // Count each removed unseen message once, even if it lives in both caches.
    const perMailbox = new Map<number, number>();
    const counted = new Set<number>();
    for (const row of [...prevMessages, ...prevUnified]) {
      if (set.has(row.id) && !row.seen && !counted.has(row.id)) {
        counted.add(row.id);
        perMailbox.set(row.mailboxId, (perMailbox.get(row.mailboxId) ?? 0) + 1);
      }
    }
    const badgeReverts: Relocation["badgeReverts"] = [];
    for (const [mailboxId, count] of perMailbox) {
      const mailbox = findMailbox(mailboxId);
      if (mailbox !== undefined) {
        badgeReverts.push({ mailbox, previous: mailbox.unreadCount });
        mailbox.unreadCount = Math.max(0, mailbox.unreadCount - count);
      }
    }

    deps.messages.value = prevMessages.filter((row) => !set.has(row.id));
    deps.unifiedItems.value = prevUnified.filter((row) => !set.has(row.id));

    const clearedSelection =
      prevSelected !== null && set.has(prevSelected) ? prevSelected : null;
    if (clearedSelection !== null) deps.selectedMessageId.value = null;

    return { prevMessages, prevUnified, badgeReverts, clearedSelection };
  }

  /** Reverts a {@link removeRows} snapshot: rows, unread badges, open message. */
  function restore(snapshot: Relocation): void {
    deps.messages.value = snapshot.prevMessages;
    deps.unifiedItems.value = snapshot.prevUnified;
    for (const { mailbox, previous } of snapshot.badgeReverts) {
      mailbox.unreadCount = previous;
    }
    if (snapshot.clearedSelection !== null) {
      deps.selectedMessageId.value = snapshot.clearedSelection;
    }
  }

  /**
   * Runs a relocation that removes rows: splice optimistically, await the
   * backend, and either raise the undo toast (success) or fully revert. A
   * `null` result means the backend was unreachable (browser preview), which is
   * a non-success: the optimistic removal is reverted, never confirmed.
   */
  async function relocate(
    ids: number[],
    run: () => ReturnType<typeof moveMessages>,
    label: (n: number) => string,
  ): Promise<void> {
    if (ids.length === 0) return;
    const snapshot = removeRows(ids);
    try {
      const outcome = await run();
      if (outcome === null) {
        restore(snapshot);
        showNotice(UNAVAILABLE);
        return;
      }
      showUndo(label(outcome.moves.length), outcome.moves);
      deps.clearSelection?.();
    } catch (error) {
      restore(snapshot);
      showNotice(errorText(error));
    }
  }

  async function archive(ids: number[]): Promise<void> {
    await relocate(ids, () => archiveMessages(ids), (n) => `Archived ${n}`);
  }

  async function remove(ids: number[]): Promise<void> {
    await relocate(ids, () => deleteToTrash(ids), (n) => `Deleted ${n}`);
  }

  async function moveTo(
    ids: number[],
    mailboxId: number,
    mailboxName: string,
  ): Promise<void> {
    await relocate(
      ids,
      () => moveMessages(ids, mailboxId),
      () => `Moved to ${mailboxName}`,
    );
  }

  async function copyTo(
    ids: number[],
    mailboxId: number,
    mailboxName: string,
  ): Promise<void> {
    if (ids.length === 0) return;
    // Copy leaves the source in place, so there is nothing to revert — but the
    // browser preview cannot copy, so never claim success there (copy resolves
    // to `null` on both a real success and a missing bridge; only the bridge
    // check distinguishes them).
    if (!isTauri()) {
      showNotice(UNAVAILABLE);
      return;
    }
    try {
      await copyMessages(ids, mailboxId);
      showNotice(`Copied to ${mailboxName}`);
    } catch (error) {
      showNotice(errorText(error));
    }
  }

  /** Marks a set of messages read, optimistically, reverting on failure. */
  async function markRead(ids: number[]): Promise<void> {
    if (ids.length === 0) return;
    const set = new Set(ids);
    const flipped: MessageListItem[] = [];
    // Decrement each message's badge once even if it appears in both caches.
    const counted = new Set<number>();
    for (const list of [deps.messages, deps.unifiedItems]) {
      for (const row of list.value) {
        if (set.has(row.id) && !row.seen) {
          row.seen = true;
          if (!counted.has(row.id)) {
            counted.add(row.id);
            decrementUnread(row.mailboxId);
          }
          flipped.push(row);
        }
      }
    }
    if (flipped.length === 0) return;
    try {
      await markMessagesRead(ids, true);
    } catch {
      for (const row of flipped) row.seen = false;
      deps.reload();
    }
  }

  async function undo(): Promise<void> {
    const state = undoState.value;
    if (state === null) return;
    dismissUndo();
    try {
      await undoMove(state.moves);
    } finally {
      deps.reload();
    }
  }

  /** Opens the folder picker for a move or copy of `ids`. */
  function requestMove(ids: number[]): void {
    if (ids.length > 0) deps.dialogs.open("moveToFolder", { ids, mode: "move" });
  }

  function requestCopy(ids: number[]): void {
    if (ids.length > 0) deps.dialogs.open("moveToFolder", { ids, mode: "copy" });
  }

  /** Completes a move/copy once the picker resolves a target folder. */
  function onFolderPicked(picked: FolderPicked): void {
    if (picked.mode === "move") {
      void moveTo(picked.ids, picked.mailboxId, picked.mailboxName);
    } else {
      void copyTo(picked.ids, picked.mailboxId, picked.mailboxName);
    }
  }

  /** Dispatches a bulk/keyboard/context-menu action over `ids`. */
  function dispatch(action: BulkAction, ids: number[]): void {
    if (ids.length === 0) return;
    if (action === "archive") void archive(ids);
    else if (action === "delete") void remove(ids);
    else if (action === "move") requestMove(ids);
    else if (action === "copy") requestCopy(ids);
    else void markRead(ids);
  }

  // Clear pending timers on teardown/HMR so a fired callback never mutates
  // disposed state (`failSilently` keeps direct, scope-less unit calls quiet).
  onScopeDispose(() => {
    if (undoTimer !== null) clearTimeout(undoTimer);
    if (noticeTimer !== null) clearTimeout(noticeTimer);
  }, true);

  return {
    undoState,
    notice,
    archive,
    remove,
    moveTo,
    copyTo,
    markRead,
    undo,
    dismissUndo,
    requestMove,
    requestCopy,
    onFolderPicked,
    dispatch,
  };
}

/** A user-facing string for a rejected command (backend errors are strings). */
function errorText(error: unknown): string {
  return typeof error === "string" ? error : "The action could not be completed.";
}
