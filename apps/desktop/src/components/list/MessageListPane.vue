<script setup lang="ts">
import { computed } from "vue";

import type { MessageListItem as MessageListItemModel } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageListItem from "./MessageListItem.vue";

const props = defineProps<{
  messages: MessageListItemModel[];
  selectedMessageId: number | null;
}>();

const emit = defineEmits<{
  select: [messageId: number];
  toggleFlag: [messageId: number];
  markAllRead: [];
}>();

const hasUnread = computed(() => props.messages.some((m) => !m.seen));
</script>

<template>
  <section
    class="list"
    aria-label="Message list"
  >
    <div
      v-if="messages.length > 0"
      class="toolbar"
    >
      <button
        type="button"
        class="mark-read"
        :disabled="!hasUnread"
        @click="emit('markAllRead')"
      >
        Mark all read
      </button>
    </div>
    <EmptyState
      v-if="messages.length === 0"
      title="No messages"
      detail="Messages appear here once an account is set up and synchronized."
    />
    <ul v-else>
      <MessageListItem
        v-for="message in messages"
        :key="message.id"
        :message="message"
        :selected="message.id === selectedMessageId"
        @select="emit('select', $event)"
        @toggle-flag="emit('toggleFlag', $event)"
      />
    </ul>
  </section>
</template>

<style scoped>
.list {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}

.toolbar {
  display: flex;
  justify-content: flex-end;
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--color-border);
  position: sticky;
  top: 0;
  background: var(--color-bg);
  z-index: 1;
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
</style>
