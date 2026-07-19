/**
 * Visible-row model for the message list.
 *
 * The list renders a flat sequence of rows regardless of threading mode: in
 * flat mode every row is a {@link MessageRow}; in threaded mode a
 * {@link ThreadRow} stands in for each conversation and, when expanded, is
 * followed by one {@link MessageRow} per member message. Each row carries the
 * tagged selection id ({@link messageRowId} / {@link threadRowId}) the shared
 * {@link "../../composables/useMessageSelection"} model keys on, so a thread
 * row and a message row that share a numeric id never collide.
 */
import type { MessageListItem } from "../../types/mail";
import { messageRowId, threadRowId } from "../../types/selection";
import type { Thread } from "./threadAssembly";

/** A single-message row. */
export interface MessageRow {
  kind: "message";
  /** Tagged selection id, e.g. `"m:123"`. */
  rowId: string;
  /** The rendered message. */
  message: MessageListItem;
  /** Nesting depth: 0 for a flat/top-level row, 1 for a reply under a thread. */
  depth: number;
}

/** A conversation-thread header row (collapsed or expanded). */
export interface ThreadRow {
  kind: "thread";
  /** Tagged selection id, e.g. `"t:5"`. */
  rowId: string;
  /** Stable numeric thread id (the lowest member message id). */
  threadId: number;
  /** Newest message in the thread; shown as the row's headline. */
  latest: MessageListItem;
  /** Member message ids, newest first. */
  messageIds: number[];
  /** Total number of messages in the thread. */
  count: number;
  /** Number of unread (unseen) messages in the thread. */
  unread: number;
  /** Whether the thread is expanded (its members render beneath it). */
  expanded: boolean;
}

/** Any row shown in the list. */
export type VisibleRow = MessageRow | ThreadRow;

/** Builds a flat message row for `message` at `depth`. */
export function toMessageRow(message: MessageListItem, depth = 0): MessageRow {
  return {
    kind: "message",
    rowId: messageRowId(message.id),
    message,
    depth,
  };
}

/** Builds a collapsed/expanded thread header row from an assembled thread. */
export function toThreadRow(thread: Thread, expanded: boolean): ThreadRow {
  return {
    kind: "thread",
    rowId: threadRowId(thread.id),
    threadId: thread.id,
    latest: thread.latest,
    messageIds: thread.messages.map((m) => m.id),
    count: thread.count,
    unread: thread.unread,
    expanded,
  };
}
