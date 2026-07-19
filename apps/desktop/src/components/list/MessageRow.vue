<script setup lang="ts">
/**
 * One message row in the virtualized list (`role="option"`).
 *
 * A plain click activates the row (opens the message); Ctrl/Cmd- and
 * Shift-clicks, and the leading checkbox, drive the shared multi-selection via
 * the `toggle` / `range` events. The star and checkbox carry `tabindex="-1"`
 * so keyboard focus stays on the owning listbox (roving `aria-activedescendant`
 * rather than a tab stop per row).
 */
import { computed } from "vue";

import type { MessageListItem } from "../../types/mail";
import TagChips from "../message/TagChips.vue";

const props = withDefaults(
  defineProps<{
    message: MessageListItem;
    /** DOM id used as the listbox's `aria-activedescendant` target. */
    rowDomId: string;
    /** Whether this row is in the multi-selection (checkbox + highlight). */
    checked: boolean;
    /** Whether this row is the open message (reading pane). */
    current: boolean;
    /** Whether this row is the keyboard-active descendant. */
    active: boolean;
    /** Account display name shown as a tertiary indicator (unified inbox). */
    accountName?: string | null;
    /** Nesting depth: 1 indents a reply shown under an expanded thread. */
    depth?: number;
  }>(),
  { accountName: null, depth: 0 },
);

const emit = defineEmits<{
  activate: [];
  toggle: [];
  range: [];
  toggleFlag: [messageId: number];
  context: [event: MouseEvent];
}>();

function onItemClick(event: MouseEvent): void {
  if (event.shiftKey) emit("range");
  else if (event.ctrlKey || event.metaKey) emit("toggle");
  else emit("activate");
}

function formatDate(unix: number | null): string {
  if (unix === null) return "";
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

const date = computed(() => formatDate(props.message.dateUnix));
</script>

<template>
  <div
    :id="rowDomId"
    role="option"
    class="wrap"
    :class="{ checked, active, child: depth > 0 }"
    :aria-selected="checked"
    :aria-current="current ? 'true' : undefined"
    @contextmenu.prevent="emit('context', $event)"
  >
    <input
      type="checkbox"
      class="check"
      tabindex="-1"
      :checked="checked"
      :aria-label="`Select ${message.subject ?? 'this message'}`"
      @click.stop
      @change="emit('toggle')"
    >
    <button
      type="button"
      class="star"
      tabindex="-1"
      :class="{ on: message.flagged }"
      :aria-pressed="message.flagged"
      :aria-label="message.flagged ? 'Remove star' : 'Add star'"
      @click.stop="emit('toggleFlag', message.id)"
    >
      <span aria-hidden="true">{{ message.flagged ? "★" : "☆" }}</span>
    </button>
    <button
      type="button"
      class="item"
      tabindex="-1"
      :class="{ selected: current, unseen: !message.seen }"
      @click="onItemClick"
    >
      <span class="line">
        <span class="from">{{ message.fromName || message.fromAddress }}</span>
        <span
          v-if="accountName"
          class="account"
        >{{ accountName }}</span>
        <time class="date">{{ date }}</time>
      </span>
      <span class="subject">{{ message.subject ?? "(no subject)" }}</span>
      <span
        v-if="message.preview"
        class="preview"
      >{{ message.preview }}</span>
      <TagChips
        v-if="message.tags.length > 0"
        class="tags"
        :tags="message.tags"
        :label="`Tags on ${message.subject ?? 'this message'}`"
      />
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

.wrap.child .item {
  padding-left: var(--space-5);
}

.check {
  flex-shrink: 0;
  margin: 0 var(--space-1) 0 var(--space-2);
  align-self: center;
  accent-color: var(--color-accent);
}

.star {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 var(--space-1);
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
  transition: color var(--transition-fast);
}

.star:hover {
  color: var(--color-text-secondary);
}

.star.on {
  color: var(--color-accent);
}

.star:focus-visible {
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

.unseen .from,
.unseen .subject {
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
  max-width: 45%;
}

.date {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.subject {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tags {
  margin-top: 2px;
}
</style>
