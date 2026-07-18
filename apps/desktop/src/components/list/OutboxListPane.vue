<script setup lang="ts">
import type { OutboxItemDto, OutboxState } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";

defineProps<{
  items: OutboxItemDto[];
}>();

const emit = defineEmits<{
  retry: [outboxId: number];
}>();

const STATE_LABEL: Record<OutboxState, string> = {
  pending: "Pending",
  sending: "Sending",
  sent: "Sent",
  failed: "Failed",
};
</script>

<template>
  <section
    class="list"
    aria-label="Outbox"
  >
    <EmptyState
      v-if="items.length === 0"
      title="Outbox empty"
      detail="Messages waiting to be delivered appear here."
    />
    <ul v-else>
      <li
        v-for="item in items"
        :key="item.id"
        class="item"
      >
        <div class="row">
          <span class="subject">{{ item.subject || "(no subject)" }}</span>
          <span
            class="badge"
            :class="`state-${item.state}`"
          >
            {{ STATE_LABEL[item.state] }}
          </span>
        </div>
        <span class="to">{{ item.toSummary }}</span>
        <span
          v-if="item.attempts > 0"
          class="attempts"
        >
          {{ item.attempts }} attempt{{ item.attempts === 1 ? "" : "s" }}
        </span>
        <p
          v-if="item.lastError"
          class="last-error"
        >
          {{ item.lastError }}
        </p>
        <button
          v-if="item.state === 'failed'"
          type="button"
          class="retry"
          @click="emit('retry', item.id)"
        >
          Retry
        </button>
      </li>
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

.item {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
}

.row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--space-2);
}

.subject {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.badge {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  padding: 1px var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
}

.badge.state-failed {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.badge.state-sent {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.to {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.attempts {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.last-error {
  font-size: var(--text-xs);
  color: var(--color-danger);
  overflow-wrap: anywhere;
}

.retry {
  align-self: flex-start;
  margin-top: var(--space-1);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  transition: background var(--transition-fast);
}

.retry:hover {
  background: var(--color-bg-hover);
}
</style>
