<script setup lang="ts">
/**
 * A conversation-thread header row (`role="option"`).
 *
 * It shows the latest message in the thread plus a message count and an unread
 * rollup, with a leading twisty that expands/collapses the thread's older
 * replies. A plain click activates the row (opens the latest message);
 * Ctrl/Cmd- and Shift-clicks and the checkbox drive the shared multi-selection,
 * where the whole thread stands in for all its member messages.
 */
import { computed } from "vue";

import type { ThreadRow } from "./rows";

const props = defineProps<{
  row: ThreadRow;
  /** DOM id used as the listbox's `aria-activedescendant` target. */
  rowDomId: string;
  /** Whether this thread is in the multi-selection. */
  checked: boolean;
  /** Whether the thread's latest message is the open message. */
  current: boolean;
  /** Whether this row is the keyboard-active descendant. */
  active: boolean;
  /** Account display name shown as a tertiary indicator (unified inbox). */
  accountName?: string | null;
}>();

const emit = defineEmits<{
  activate: [];
  toggle: [];
  range: [];
  expand: [];
  toggleFlag: [messageId: number];
}>();

function onBodyClick(event: MouseEvent): void {
  if (event.shiftKey) emit("range");
  else if (event.ctrlKey || event.metaKey) emit("toggle");
  else emit("activate");
}

const latest = computed(() => props.row.latest);

function formatDate(unix: number | null): string {
  if (unix === null) return "";
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

const date = computed(() => formatDate(latest.value.dateUnix));
const countLabel = computed(() =>
  props.row.unread > 0
    ? `${props.row.count} messages, ${props.row.unread} unread`
    : `${props.row.count} messages`,
);
</script>

<template>
  <div
    :id="rowDomId"
    role="option"
    class="wrap"
    :class="{ checked, active, unread: row.unread > 0 }"
    :aria-selected="checked"
    :aria-current="current ? 'true' : undefined"
    :aria-expanded="row.expanded"
  >
    <input
      type="checkbox"
      class="check"
      tabindex="-1"
      :checked="checked"
      :aria-label="`Select conversation ${latest.subject ?? '(no subject)'}`"
      @click.stop
      @change="emit('toggle')"
    >
    <button
      type="button"
      class="twisty"
      tabindex="-1"
      :aria-label="row.expanded ? 'Collapse conversation' : 'Expand conversation'"
      :aria-pressed="row.expanded"
      @click.stop="emit('expand')"
    >
      <span aria-hidden="true">{{ row.expanded ? "▾" : "▸" }}</span>
    </button>
    <button
      type="button"
      class="item"
      tabindex="-1"
      :class="{ selected: current }"
      @click="onBodyClick"
    >
      <span class="line">
        <span class="from">{{ latest.fromName || latest.fromAddress }}</span>
        <span
          v-if="accountName"
          class="account"
        >{{ accountName }}</span>
        <time class="date">{{ date }}</time>
      </span>
      <span class="line">
        <span class="subject">{{ latest.subject ?? "(no subject)" }}</span>
        <span
          class="count"
          :aria-label="countLabel"
        >{{ row.count }}<span
          v-if="row.unread > 0"
          class="dot"
          aria-hidden="true"
        >•{{ row.unread }}</span></span>
      </span>
    </button>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  align-items: stretch;
  height: var(--row-height);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
  overflow: hidden;
}

.wrap.checked {
  background: var(--color-accent-subtle);
}

.wrap.active {
  box-shadow: inset 2px 0 0 var(--color-accent);
}

.check {
  flex-shrink: 0;
  margin: 0 var(--space-1) 0 var(--space-2);
  align-self: center;
  accent-color: var(--color-accent);
}

.twisty {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 var(--space-1);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  transition: color var(--transition-fast);
}

.twisty:hover {
  color: var(--color-text);
}

.twisty:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: -2px;
}

.item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
  text-align: left;
  padding: var(--space-2) var(--space-3) var(--space-2) var(--space-1);
  transition: background var(--transition-fast);
}

.item:hover {
  background: var(--color-bg-hover);
}

.item.selected {
  background: var(--color-bg-selected);
}

.line {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--space-2);
}

.from {
  flex: 1;
  min-width: 0;
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.unread .from,
.unread .subject {
  color: var(--color-text);
  font-weight: var(--weight-semibold);
}

.account {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 40%;
}

.date {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.subject {
  flex: 1;
  min-width: 0;
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.count {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  background: var(--color-bg-hover);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-1);
}

.dot {
  color: var(--color-accent);
  margin-left: 2px;
}
</style>
