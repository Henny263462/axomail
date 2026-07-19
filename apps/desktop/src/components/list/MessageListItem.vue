<script setup lang="ts">
import { computed } from "vue";

import type { MessageListItem } from "../../types/mail";

const props = withDefaults(
  defineProps<{
    message: MessageListItem;
    selected: boolean;
    /** Account display name shown as a tertiary indicator (unified inbox). */
    accountName?: string | null;
  }>(),
  { accountName: null },
);

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
}>();

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
  <li class="wrap">
    <button
      type="button"
      class="star"
      :class="{ on: message.flagged }"
      :aria-pressed="message.flagged"
      :aria-label="message.flagged ? 'Remove star' : 'Add star'"
      @click="emit('toggleFlag', message.id)"
    >
      <span aria-hidden="true">{{ message.flagged ? "★" : "☆" }}</span>
    </button>
    <button
      type="button"
      class="item"
      :class="{ selected, unseen: !message.seen }"
      :aria-current="selected ? 'true' : undefined"
      @click="emit('select', message.id)"
    >
      <span class="row">
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
    </button>
  </li>
</template>

<style scoped>
.wrap {
  display: flex;
  align-items: stretch;
  border-bottom: 1px solid var(--color-border);
}

.star {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 var(--space-1) 0 var(--space-2);
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

.row {
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
</style>
