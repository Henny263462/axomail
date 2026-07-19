/**
 * Shared list-selection contract.
 *
 * Wave 3 tracks 3.2 (bulk actions) and 3.4 (virtualized / threaded list) both
 * operate on a multi-row selection, so they share one selection model rather
 * than each inventing its own notion of row identity. This module defines the
 * identity and gesture types; {@link "../composables/useMessageSelection"}
 * implements the selection set over them.
 *
 * ## Row identity
 *
 * The selection is a set over a **stable row identity**. Today the message list
 * is flat and every row is a single message, so a row's identity is its numeric
 * message id ({@link RowId} = `number`) and the flat list passes ids straight
 * through — {@link useMessageSelection} is generic and defaults to `number`.
 *
 * Track 3.4 introduces threaded rows: a collapsed thread row stands in for many
 * messages. To keep identities coherent — and to keep a thread row from
 * colliding with a message row that happens to share a numeric id — that track
 * widens the row id to a *tagged string* built with {@link messageRowId} /
 * {@link threadRowId} (`"m:123"` vs `"t:123"`). The selection algorithm never
 * inspects the id beyond equality and list position, so widening the type
 * requires no change to the composable: the caller picks the id type and feeds
 * the ordered id list.
 */

/**
 * Stable identity of a selectable list row. `number` (a raw message id) today;
 * a track may widen to a tagged `string` (see the module docs) for threaded
 * rows.
 */
export type RowId = number | string;

/**
 * Modifiers that shape a click-driven selection gesture. The list component
 * that owns the rows maps them from the originating pointer event's keyboard
 * state (Ctrl/Cmd → {@link toggle}, Shift → {@link range}); a plain click
 * carries neither and replaces the selection with the clicked row.
 */
export interface SelectionGesture {
  /** Ctrl/Cmd-click: flip just this row's membership, preserving the rest. */
  toggle?: boolean;
  /** Shift-click: select the contiguous range from the anchor to this row. */
  range?: boolean;
}

/** Canonical selection key for a single message row (track 3.4 threaded list). */
export function messageRowId(messageId: number): string {
  return `m:${messageId}`;
}

/** Canonical selection key for a collapsed thread row (track 3.4 threaded list). */
export function threadRowId(threadId: number): string {
  return `t:${threadId}`;
}
