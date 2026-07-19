<script setup lang="ts">
import { useWorkOffline } from "../../composables/useWorkOffline";
import { formatScheduleTime } from "../../lib/sendLater";
import type { OutboxItemDto, OutboxState } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";

defineProps<{
  items: OutboxItemDto[];
}>();

// The outbox reflects the shared Work Offline flag: while offline, a plain
// queued row is labelled "Queued (offline)" to explain why it has not sent.
const { offline } = useWorkOffline();

const emit = defineEmits<{
  retry: [outboxId: number];
  "send-now": [outboxId: number];
  "cancel-schedule": [outboxId: number];
}>();

const STATE_LABEL: Record<OutboxState, string> = {
  pending: "Pending",
  sending: "Sending",
  sent: "Sent",
  failed: "Failed",
};

/** True when a pending row is scheduled to be sent at a future time. */
function isScheduled(item: OutboxItemDto): boolean {
  return (
    item.state === "pending" &&
    item.scheduledUnix != null &&
    item.scheduledUnix > Math.floor(Date.now() / 1000)
  );
}

/** Localized scheduled-send time for a row, or an empty string. */
function scheduleLabel(item: OutboxItemDto): string {
  return item.scheduledUnix != null ? formatScheduleTime(item.scheduledUnix) : "";
}

/**
 * Badge text for a row: an immediately-eligible queued row reads as
 * "Queued (offline)" while the app is offline, so the user understands it is
 * waiting on the connection rather than stuck.
 */
function badgeLabel(item: OutboxItemDto): string {
  if (offline.value && item.state === "pending" && !isScheduled(item)) {
    return "Queued (offline)";
  }
  return STATE_LABEL[item.state];
}
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
            {{ badgeLabel(item) }}
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
        <span
          v-if="isScheduled(item)"
          class="scheduled"
        >
          Scheduled for {{ scheduleLabel(item) }}
        </span>
        <div
          v-if="isScheduled(item)"
          class="schedule-actions"
        >
          <button
            type="button"
            class="retry"
            @click="emit('send-now', item.id)"
          >
            Send now
          </button>
          <button
            type="button"
            class="retry"
            @click="emit('cancel-schedule', item.id)"
          >
            Cancel schedule
          </button>
        </div>
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

.scheduled {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-accent);
}

.schedule-actions {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-1);
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
