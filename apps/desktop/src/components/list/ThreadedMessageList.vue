<script setup lang="ts">
/**
 * The virtualized, threadable, filterable message list.
 *
 * Shared by the mailbox and unified-inbox panes. It renders only the rows in
 * the viewport (fixed-height windowing, {@link useVirtualList}), groups the
 * loaded page into conversations when threading is on, filters/sorts it
 * client-side, and exposes the shared thread-aware multi-selection
 * ({@link useMessageListView}). Keyboard navigation roves an
 * `aria-activedescendant` over the `role="listbox"`; the selection's message
 * ids bubble up via `selectionChange` for bulk actions (track 3.2). New pages
 * load through `loadMore` when the window nears the end of the loaded rows.
 */
import { computed, toRef, useId, watch } from "vue";

import type { AccountSummary, MessageListItem, TagChip } from "../../types/mail";
import { useListNavigation } from "../../composables/useListNavigation";
import { useMessageListView } from "../../composables/useMessageListView";
import { useVirtualList } from "../../composables/useVirtualList";
import EmptyState from "../common/EmptyState.vue";
import MessageRow from "./MessageRow.vue";
import QuickFilterBar from "./QuickFilterBar.vue";
import ThreadRowItem from "./ThreadRow.vue";
import type { VisibleRow } from "./rows";

/** Uniform row height in pixels (kept in sync with the row components' CSS). */
const ROW_HEIGHT = 76;
/** Load the next page when the window's end is within this many rows of the end. */
const LOAD_MORE_THRESHOLD = 6;

const props = withDefaults(
  defineProps<{
    messages: MessageListItem[];
    selectedMessageId: number | null;
    threaded: boolean;
    /** Accessible name for the list region (e.g. "Message list"). */
    listLabel: string;
    /** Empty-state detail shown when the source list is empty. */
    emptyDetail: string;
    /** Whether the "Mark all read" header action is offered. */
    showMarkAllRead?: boolean;
    /** Whether another page can be loaded (drives infinite scroll). */
    hasMore?: boolean;
    /** Whether a next-page fetch is in flight (suppresses re-triggering). */
    loadingMore?: boolean;
    /** Accounts, used to label rows with their account (unified inbox). */
    accounts?: AccountSummary[];
    /** Whether each row shows its account name (unified inbox). */
    showAccount?: boolean;
  }>(),
  {
    showMarkAllRead: false,
    hasMore: false,
    loadingMore: false,
    accounts: () => [],
    showAccount: false,
  },
);

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
  markAllRead: [];
  loadMore: [];
  selectionChange: [messageIds: number[]];
  "update:threaded": [value: boolean];
}>();

const uid = useId();
const view = useMessageListView({
  messages: toRef(props, "messages"),
  threaded: toRef(props, "threaded"),
});

const virtual = useVirtualList({
  count: computed(() => view.visibleRows.value.length),
  rowHeight: ROW_HEIGHT,
});
// A function ref assigns the scroll element into the composable's element ref
// (a template `ref="virtual.scroller"` string could not target a nested ref).
function setScroller(el: unknown): void {
  virtual.scroller.value = (el as HTMLElement | null) ?? null;
}

const hasUnread = computed(() => props.messages.some((m) => !m.seen));
const selectionCount = computed(() => view.selection.count.value);

/** Tags present in the loaded set, de-duplicated, for the tag picker. */
const availableTags = computed<TagChip[]>(() => {
  const byId = new Map<number, TagChip>();
  for (const message of props.messages) {
    for (const tag of message.tags) if (!byId.has(tag.id)) byId.set(tag.id, tag);
  }
  return [...byId.values()];
});

const accountNames = computed(() => {
  const map = new Map<number, string>();
  for (const account of props.accounts) map.set(account.id, account.displayName);
  return map;
});

function accountName(message: MessageListItem): string | null {
  if (!props.showAccount) return null;
  const accountId = (message as { accountId?: number }).accountId;
  if (accountId === undefined) return null;
  return accountNames.value.get(accountId) ?? `Account ${accountId}`;
}

/** DOM id for a row, used as the listbox's `aria-activedescendant` target. */
function domId(rowId: string): string {
  return `${uid}-${rowId.replace(":", "-")}`;
}

// Keyboard + pointer navigation: roving active descendant + click gestures.
const nav = useListNavigation({
  visibleRows: view.visibleRows,
  selection: view.selection,
  toggleThread: view.toggleThread,
  scrollToIndex: virtual.scrollToIndex,
  onOpen: (messageId) => emit("select", messageId),
});
const { activeIndex, activeRow, activate, toggle, range, onKeydown } = nav;

const activeDomId = computed(() =>
  activeRow.value ? domId(activeRow.value.rowId) : undefined,
);

/** The windowed slice actually rendered, tagged with its absolute index. */
const windowRows = computed(() => {
  const { start, end } = virtual.window.value;
  const rows: { row: VisibleRow; index: number }[] = [];
  for (let index = start; index < end; index += 1) {
    const row = view.visibleRows.value[index];
    if (row) rows.push({ row, index });
  }
  return rows;
});

// Surface the resolved selection (message ids) for bulk actions (track 3.2).
watch(
  view.selectedMessageIds,
  (ids) => emit("selectionChange", [...ids]),
  { flush: "post" },
);

// Infinite scroll: pull the next page once the window nears the loaded end.
watch(
  () => virtual.window.value.end,
  (end) => {
    if (!props.hasMore || props.loadingMore) return;
    if (end >= view.visibleRows.value.length - LOAD_MORE_THRESHOLD) {
      emit("loadMore");
    }
  },
);
</script>

<template>
  <section
    class="list"
    :aria-label="listLabel"
  >
    <div
      v-if="messages.length > 0 && (showMarkAllRead || selectionCount > 0)"
      class="toolbar"
    >
      <span
        v-if="selectionCount > 0"
        class="selection-count"
        role="status"
      >{{ selectionCount }} selected</span>
      <button
        v-if="selectionCount > 0"
        type="button"
        class="clear-selection"
        @click="view.selection.clear()"
      >
        Clear
      </button>
      <span class="spacer" />
      <button
        v-if="showMarkAllRead"
        type="button"
        class="mark-read"
        :disabled="!hasUnread"
        @click="emit('markAllRead')"
      >
        Mark all read
      </button>
    </div>

    <QuickFilterBar
      v-if="messages.length > 0"
      :filter="view.filter.value"
      :sort="view.sort.value"
      :threaded="threaded"
      :available-tags="availableTags"
      @update:filter="view.filter.value = $event"
      @update:sort="view.sort.value = $event"
      @update:threaded="emit('update:threaded', $event)"
    />

    <EmptyState
      v-if="messages.length === 0"
      title="No messages"
      :detail="emptyDetail"
    />
    <EmptyState
      v-else-if="view.visibleRows.value.length === 0"
      title="No matching messages"
      detail="No messages match the current filter."
    />
    <div
      v-else
      :ref="setScroller"
      class="scroller"
      role="listbox"
      aria-multiselectable="true"
      :aria-label="listLabel"
      :aria-activedescendant="activeDomId"
      tabindex="0"
      :style="{ '--row-height': ROW_HEIGHT + 'px' }"
      @scroll="virtual.onScroll"
      @keydown="onKeydown"
    >
      <div
        class="sizer"
        :style="{ height: virtual.window.value.totalHeight + 'px' }"
      >
        <div
          class="rows"
          :style="{ transform: `translateY(${virtual.window.value.offsetY}px)` }"
        >
          <template
            v-for="entry in windowRows"
            :key="entry.row.rowId"
          >
            <ThreadRowItem
              v-if="entry.row.kind === 'thread'"
              :row="entry.row"
              :row-dom-id="domId(entry.row.rowId)"
              :checked="view.selection.isSelected(entry.row.rowId)"
              :current="entry.row.latest.id === selectedMessageId"
              :active="entry.index === activeIndex"
              :account-name="accountName(entry.row.latest)"
              @activate="activate(entry.row, entry.index)"
              @toggle="toggle(entry.row, entry.index)"
              @range="range(entry.row, entry.index)"
              @expand="view.toggleThread(entry.row.threadId)"
              @toggle-flag="emit('toggleFlag', $event)"
            />
            <MessageRow
              v-else
              :message="entry.row.message"
              :row-dom-id="domId(entry.row.rowId)"
              :checked="view.selection.isSelected(entry.row.rowId)"
              :current="entry.row.message.id === selectedMessageId"
              :active="entry.index === activeIndex"
              :account-name="accountName(entry.row.message)"
              :depth="entry.row.depth"
              @activate="activate(entry.row, entry.index)"
              @toggle="toggle(entry.row, entry.index)"
              @range="range(entry.row, entry.index)"
              @toggle-flag="emit('toggleFlag', $event)"
            />
          </template>
        </div>
      </div>
      <p
        v-if="loadingMore"
        class="loading"
        role="status"
      >
        Loading more…
      </p>
    </div>
  </section>
</template>

<style scoped>
.list {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
}

.spacer {
  flex: 1;
}

.selection-count {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.clear-selection {
  font-size: var(--text-xs);
  color: var(--color-accent);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.clear-selection:hover {
  background: var(--color-bg-hover);
}

.mark-read {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-accent);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.mark-read:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.mark-read:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.scroller {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  position: relative;
}

.scroller:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: -2px;
}

.sizer {
  position: relative;
  width: 100%;
}

.rows {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  will-change: transform;
}

.loading {
  position: sticky;
  bottom: 0;
  padding: var(--space-2);
  text-align: center;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  background: var(--color-bg);
}
</style>
