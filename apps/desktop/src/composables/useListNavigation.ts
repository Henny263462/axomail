/**
 * Keyboard and pointer navigation for the message listbox: a roving active
 * index (surfaced as the `aria-activedescendant`), plain/Ctrl/Shift click
 * gestures, and the arrow/Home/End/Space/Enter key map. It drives the shared
 * selection and opens the active message; thread rows expand/collapse.
 *
 * Extracted from `ThreadedMessageList.vue` so the component stays under the
 * file-size gate.
 */
import { computed, ref, watch, type ComputedRef } from "vue";

import type { VisibleRow } from "../components/list/rows";
import type { MessageSelection } from "./useMessageSelection";

interface Deps {
  /** Rows in visible order (the navigation domain). */
  visibleRows: ComputedRef<VisibleRow[]>;
  /** The shared multi-selection. */
  selection: MessageSelection<string>;
  /** Expands/collapses a thread row. */
  toggleThread: (threadId: number) => void;
  /** Scrolls a row index into view (keyboard navigation). */
  scrollToIndex: (index: number) => void;
  /** Opens a message in the reading pane. */
  onOpen: (messageId: number) => void;
}

export function useListNavigation(deps: Deps) {
  const activeIndex = ref(0);
  const activeRow = computed<VisibleRow | null>(
    () => deps.visibleRows.value[activeIndex.value] ?? null,
  );

  // Keep the active index inside the (reloaded/filtered) row set.
  watch(
    () => deps.visibleRows.value.length,
    (length) => {
      if (length === 0) activeIndex.value = 0;
      else if (activeIndex.value > length - 1) activeIndex.value = length - 1;
    },
  );

  function openRow(row: VisibleRow): void {
    deps.onOpen(row.kind === "thread" ? row.latest.id : row.message.id);
  }

  function activate(row: VisibleRow, index: number): void {
    activeIndex.value = index;
    deps.selection.selectOnly(row.rowId);
    openRow(row);
  }

  function toggle(row: VisibleRow, index: number): void {
    activeIndex.value = index;
    deps.selection.select(row.rowId, { toggle: true });
  }

  function range(row: VisibleRow, index: number): void {
    activeIndex.value = index;
    deps.selection.select(row.rowId, { range: true });
  }

  function setActive(index: number, extend: boolean): void {
    activeIndex.value = index;
    deps.scrollToIndex(index);
    const row = deps.visibleRows.value[index];
    if (!row) return;
    if (extend) {
      deps.selection.select(row.rowId, { range: true });
    } else {
      deps.selection.selectOnly(row.rowId);
      openRow(row);
    }
  }

  function moveActive(delta: number, extend: boolean): void {
    const length = deps.visibleRows.value.length;
    if (length === 0) return;
    const next = Math.max(0, Math.min(length - 1, activeIndex.value + delta));
    setActive(next, extend);
  }

  function onKeydown(event: KeyboardEvent): void {
    const row = activeRow.value;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveActive(1, event.shiftKey);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveActive(-1, event.shiftKey);
        break;
      case "Home":
        event.preventDefault();
        setActive(0, event.shiftKey);
        break;
      case "End":
        event.preventDefault();
        setActive(deps.visibleRows.value.length - 1, event.shiftKey);
        break;
      case " ":
        if (row) {
          event.preventDefault();
          deps.selection.select(row.rowId, { toggle: true });
        }
        break;
      case "Enter":
        if (row) {
          event.preventDefault();
          if (row.kind === "thread") deps.toggleThread(row.threadId);
          else deps.onOpen(row.message.id);
        }
        break;
      case "ArrowRight":
        if (row?.kind === "thread" && !row.expanded) {
          event.preventDefault();
          deps.toggleThread(row.threadId);
        }
        break;
      case "ArrowLeft":
        if (row?.kind === "thread" && row.expanded) {
          event.preventDefault();
          deps.toggleThread(row.threadId);
        }
        break;
      default:
        break;
    }
  }

  return { activeIndex, activeRow, activate, toggle, range, onKeydown };
}
