/**
 * The message-list view model: quick-filter + sort + conversation threading
 * over a loaded page, producing the flat sequence of visible rows the list
 * renders and the thread-aware multi-selection that feeds bulk actions.
 *
 * It composes the shared {@link useMessageSelection} contract (one selection
 * model, keyed on tagged row ids) rather than inventing its own, and exposes
 * `selectedMessageIds` — the underlying message ids of the selection, with a
 * selected thread expanded to all its members — for track 3.2 to consume.
 */
import { computed, ref, type ComputedRef, type Ref } from "vue";

import type { MessageListItem } from "../types/mail";
import {
  defaultSort,
  emptyFilter,
  filterMessages,
  isFilterActive,
  sortMessages,
  compareMessages,
  type QuickFilter,
  type SortSpec,
} from "../components/list/listFilter";
import {
  toMessageRow,
  toThreadRow,
  type VisibleRow,
} from "../components/list/rows";
import { assembleThreads } from "../components/list/threadAssembly";
import { useMessageSelection, type MessageSelection } from "./useMessageSelection";

/** Options for {@link useMessageListView}. */
export interface MessageListViewOptions {
  /** The loaded page(s) of message envelopes (the list's data source). */
  messages: Ref<MessageListItem[]>;
  /** Whether conversations are grouped into threads (persisted preference). */
  threaded: Ref<boolean>;
}

/** The reactive view model returned by {@link useMessageListView}. */
export interface MessageListView {
  /** The active quick-filter facets (two-way; the filter bar mutates it). */
  filter: Ref<QuickFilter>;
  /** The active sort (two-way; the sort control mutates it). */
  sort: Ref<SortSpec>;
  /** Rows to render, in visible order (flat rows, or thread rows + children). */
  visibleRows: ComputedRef<VisibleRow[]>;
  /** Row ids in visible order — the selection's ordering + range source. */
  rowIds: ComputedRef<string[]>;
  /** The shared multi-row selection over the tagged row ids. */
  selection: MessageSelection<string>;
  /** Message ids of the current selection (a thread expands to its members). */
  selectedMessageIds: ComputedRef<number[]>;
  /** Whether any facet is currently constraining the list. */
  filterActive: ComputedRef<boolean>;
  /** Whether a thread is expanded. */
  isThreadExpanded: (threadId: number) => boolean;
  /** Expands or collapses a thread. */
  toggleThread: (threadId: number) => void;
}

export function useMessageListView(
  options: MessageListViewOptions,
): MessageListView {
  const filter = ref<QuickFilter>(emptyFilter());
  const sort = ref<SortSpec>(defaultSort());
  const expanded = ref(new Set<number>());

  /** Filtered then sorted flat list — the basis for both modes. */
  const filtered = computed(() =>
    sortMessages(filterMessages(options.messages.value, filter.value), sort.value),
  );

  /** Conversations assembled from the filtered set, ordered by the sort. */
  const threads = computed(() => {
    if (!options.threaded.value) return [];
    return assembleThreads(filtered.value).sort((a, b) =>
      compareMessages(a.latest, b.latest, sort.value),
    );
  });

  const visibleRows = computed<VisibleRow[]>(() => {
    if (!options.threaded.value) {
      return filtered.value.map((message) => toMessageRow(message));
    }
    const rows: VisibleRow[] = [];
    for (const thread of threads.value) {
      // A lone message renders as an ordinary row — no twisty, no rollup.
      if (thread.count === 1) {
        rows.push(toMessageRow(thread.latest));
        continue;
      }
      const isExpanded = expanded.value.has(thread.id);
      rows.push(toThreadRow(thread, isExpanded));
      if (isExpanded) {
        // Children are the older replies; the latest is the thread row itself.
        for (const message of thread.messages.slice(1)) {
          rows.push(toMessageRow(message, 1));
        }
      }
    }
    return rows;
  });

  /** Which message ids each visible row stands for (for selection resolution). */
  const rowMessageIds = computed(() => {
    const map = new Map<string, number[]>();
    for (const row of visibleRows.value) {
      map.set(
        row.rowId,
        row.kind === "thread" ? row.messageIds : [row.message.id],
      );
    }
    return map;
  });

  const rowIds = computed(() => visibleRows.value.map((row) => row.rowId));
  const selection = useMessageSelection<string>(rowIds);

  const selectedMessageIds = computed<number[]>(() => {
    const seen = new Set<number>();
    const ids: number[] = [];
    for (const rowId of selection.selectedIds.value) {
      for (const messageId of rowMessageIds.value.get(rowId) ?? []) {
        if (!seen.has(messageId)) {
          seen.add(messageId);
          ids.push(messageId);
        }
      }
    }
    return ids;
  });

  const filterActive = computed(() => isFilterActive(filter.value));

  function isThreadExpanded(threadId: number): boolean {
    return expanded.value.has(threadId);
  }

  function toggleThread(threadId: number): void {
    const next = new Set(expanded.value);
    if (next.has(threadId)) next.delete(threadId);
    else next.add(threadId);
    expanded.value = next;
  }

  return {
    filter,
    sort,
    visibleRows,
    rowIds,
    selection,
    selectedMessageIds,
    filterActive,
    isThreadExpanded,
    toggleThread,
  };
}
