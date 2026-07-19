import { computed, ref, watch, type ComputedRef, type Ref } from "vue";

import type { RowId, SelectionGesture } from "../types/selection";

/**
 * The reactive selection primitive returned by {@link useMessageSelection}.
 * `Id` is the row-identity type (defaults to `number`; see `types/selection`).
 */
export interface MessageSelection<Id extends RowId> {
  /** The selected ids, in the list's current visible order. Reactive. */
  selectedIds: ComputedRef<Id[]>;
  /** Number of selected rows. Reactive; cheaper than reading `selectedIds`. */
  count: ComputedRef<number>;
  /** Whether any row is selected. Reactive. */
  isEmpty: ComputedRef<boolean>;
  /** The row a range gesture extends from, or null when nothing is anchored. */
  anchorId: Readonly<Ref<Id | null>>;
  /** Whether `id` is currently selected. */
  isSelected: (id: Id) => boolean;
  /**
   * Applies a click gesture to `id`:
   * - plain (no modifiers) → select only `id` and anchor on it;
   * - `toggle` (Ctrl/Cmd) → flip `id`'s membership and re-anchor on it;
   * - `range` (Shift) → select the contiguous run from the anchor to `id`,
   *   leaving the anchor in place so successive Shift-clicks pivot on it.
   */
  select: (id: Id, gesture?: SelectionGesture) => void;
  /** Replaces the selection with exactly `id` (a plain single-select). */
  selectOnly: (id: Id) => void;
  /** Toggles `id`'s membership (Ctrl/Cmd-click); re-anchors on `id`. */
  toggle: (id: Id) => void;
  /** Clears the whole selection and the anchor. */
  clear: () => void;
}

/**
 * A multi-row selection set over a stable row identity, shared by the Wave 3
 * bulk-actions (3.2) and threaded/virtualized-list (3.4) tracks.
 *
 * `orderedIds` is the list's current row ids in visible order — the single
 * source of truth for both range (Shift-click) resolution and reconciliation.
 * Pass a `computed` derived from the rendered list; the composable:
 *
 * - resolves Shift-click ranges by position within `orderedIds`;
 * - re-derives `selectedIds` in that same order (insertion order is never
 *   surfaced);
 * - **reconciles** whenever the list changes — ids that vanished drop out of
 *   the selection and a vanished anchor is forgotten, while rows that remain
 *   keep their selected state across a reload of the same list.
 *
 * Single-select is just the default (plain) gesture, so wiring this into a
 * single-select list is behaviour-preserving: without modifier gestures the
 * selection always holds at most the one clicked row.
 */
export function useMessageSelection<Id extends RowId = number>(
  orderedIds: Ref<readonly Id[]>,
): MessageSelection<Id> {
  const selected = ref(new Set<Id>()) as Ref<Set<Id>>;
  const anchorId = ref<Id | null>(null) as Ref<Id | null>;

  const selectedIds = computed<Id[]>(() =>
    orderedIds.value.filter((id) => selected.value.has(id)),
  );
  const count = computed(() => selected.value.size);
  const isEmpty = computed(() => selected.value.size === 0);

  function isSelected(id: Id): boolean {
    return selected.value.has(id);
  }

  function replaceWith(ids: readonly Id[]): void {
    selected.value = new Set(ids);
  }

  function selectOnly(id: Id): void {
    replaceWith([id]);
    anchorId.value = id;
  }

  function toggle(id: Id): void {
    const next = new Set(selected.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected.value = next;
    // A toggle re-anchors on the affected row, matching file-manager behaviour:
    // a following Shift-click extends from the row the user last touched.
    anchorId.value = id;
  }

  function selectRange(id: Id): void {
    const order = orderedIds.value;
    const to = order.indexOf(id);
    if (to === -1) return;
    const anchor = anchorId.value;
    const from = anchor === null ? -1 : order.indexOf(anchor);
    // With no live anchor a range gesture degrades to a plain select.
    if (from === -1) {
      selectOnly(id);
      return;
    }
    const [lo, hi] = from <= to ? [from, to] : [to, from];
    replaceWith(order.slice(lo, hi + 1));
    // Deliberately keep the anchor: extending the range the other way pivots on
    // the original origin, not on the most recent target.
  }

  function select(id: Id, gesture: SelectionGesture = {}): void {
    if (gesture.range) selectRange(id);
    else if (gesture.toggle) toggle(id);
    else selectOnly(id);
  }

  function clear(): void {
    if (selected.value.size > 0) selected.value = new Set();
    anchorId.value = null;
  }

  // Reconcile against the underlying list: drop selected ids and an anchor that
  // no longer appear. Rows still present keep their state, so reloading the same
  // page (new row objects, same ids) preserves the selection.
  watch(orderedIds, (ids) => {
    const present = new Set(ids);
    if (selected.value.size > 0) {
      let changed = false;
      const next = new Set<Id>();
      for (const id of selected.value) {
        if (present.has(id)) next.add(id);
        else changed = true;
      }
      if (changed) selected.value = next;
    }
    if (anchorId.value !== null && !present.has(anchorId.value)) {
      anchorId.value = null;
    }
  });

  return {
    selectedIds,
    count,
    isEmpty,
    anchorId,
    isSelected,
    select,
    selectOnly,
    toggle,
    clear,
  };
}
