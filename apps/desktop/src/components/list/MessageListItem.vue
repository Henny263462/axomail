<script setup lang="ts">
import { computed } from "vue";

import type { MessageListItem } from "../../types/mail";

const props = defineProps<{
  message: MessageListItem;
  selected: boolean;
}>();

const emit = defineEmits<{
  select: [messageId: number];
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
  <li>
    <button
      type="button"
      class="item"
      :class="{ selected, unseen: !message.seen }"
      :aria-current="selected ? 'true' : undefined"
      @click="emit('select', message.id)"
    >
      <span class="row">
        <span class="from">{{ message.fromName || message.fromAddress }}</span>
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
.item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
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
