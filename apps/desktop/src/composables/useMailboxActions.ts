import { ref, type Ref } from "vue";

import {
  archiveMessages,
  copyMessages,
  deleteToTrash,
  moveMessages,
  undoMove,
} from "../api/actions";
import { markMessagesRead } from "../api/messages";
import type { DialogController } from "./useDialogs";
import type { AppDialogMap } from "../components/layout/dialogs";
import type {
  AccountSummary,
  MessageListItem,
  MessageMove,
  UnifiedItemDto,
} from "../types/mail";

/** How long the undo toast stays before auto-dismissing. */
const UNDO_TIMEOUT_MS = 8000;
/** How long a transient notice (copy/error) stays. */
const NOTICE_TIMEOUT_MS = 4000;

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
  /** Reloads the current list from the backend (after undo or a failed op). */
  reload: () => void;
}

/**
 * Owns the reading-pane / list message actions (move, copy, archive,
 * delete-to-Trash) and their undo. Each relocation is applied optimistically —
 * the affected rows are spliced out of the current list at once — then pushed
 * to the backend; a failure reverts by reloading, and a success raises a
 * time-boxed undo toast whose {@link MessageMove} records move the messages
 * back. Copy leaves the source in place and is not undoable.
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

  /** Decrements the source folder's unread badge for an unseen removed row. */
  function decrementUnread(mailboxId: number): void {
    for (const account of deps.accounts.value) {
      const mailbox = account.mailboxes.find((m) => m.id === mailboxId);
      if (mailbox !== undefined) {
        mailbox.unreadCount = Math.max(0, mailbox.unreadCount - 1);
        return;
      }
    }
  }

  /** Splices `ids` out of both lists, updating badges and the open message. */
  function removeRows(ids: number[]): void {
    const set = new Set(ids);
    for (const list of [deps.messages, deps.unifiedItems]) {
      const remaining = [];
      for (const row of list.value) {
        if (set.has(row.id)) {
          if (!row.seen) decrementUnread(row.mailboxId);
        } else {
          remaining.push(row);
        }
      }
      list.value = remaining as never;
    }
    if (deps.selectedMessageId.value !== null && set.has(deps.selectedMessageId.value)) {
      deps.selectedMessageId.value = null;
    }
  }

  /**
   * Runs a relocation that removes rows: splice optimistically, await the
   * backend, and either raise the undo toast (success) or revert (failure).
   */
  async function relocate(
    ids: number[],
    run: () => ReturnType<typeof moveMessages>,
    label: (n: number) => string,
  ): Promise<void> {
    if (ids.length === 0) return;
    removeRows(ids);
    try {
      const outcome = await run();
      if (outcome !== null) showUndo(label(outcome.moves.length), outcome.moves);
    } catch (error) {
      deps.reload();
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
    for (const list of [deps.messages, deps.unifiedItems]) {
      for (const row of list.value) {
        if (set.has(row.id) && !row.seen) {
          row.seen = true;
          decrementUnread(row.mailboxId);
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
