<script setup lang="ts">
import type { MessageListItem as MessageListItemModel } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageListItem from "./MessageListItem.vue";

defineProps<{
  messages: MessageListItemModel[];
  selectedMessageId: number | null;
}>();

const emit = defineEmits<{
  select: [messageId: number];
}>();
</script>

<template>
  <section
    class="list"
    aria-label="Message list"
  >
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
      />
    </ul>
  </section>
</template>

<style scoped>
.list {
  height: 100%;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}
</style>
