<script setup lang="ts">
import type { SearchResultDto } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import { parseSnippet } from "./snippet";

const props = defineProps<{
  results: SearchResultDto[];
  /** True while a request is in flight (drives the status line). */
  searching: boolean;
  selectedMessageId: number | null;
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

function statusText(): string {
  if (props.searching && props.results.length === 0) return "Searching…";
  const count = props.results.length;
  return `${count} result${count === 1 ? "" : "s"}`;
}
</script>

<template>
  <section
    class="list"
    aria-label="Search results"
  >
    <p
      class="count"
      role="status"
      aria-live="polite"
    >
      {{ statusText() }}
    </p>
    <ul v-if="results.length > 0">
      <li
        v-for="result in results"
        :key="result.messageId"
      >
        <button
          type="button"
          class="item"
          :class="{ selected: result.messageId === selectedMessageId }"
          :aria-current="
            result.messageId === selectedMessageId ? 'true' : undefined
          "
          @click="emit('select', result.messageId)"
        >
          <span class="row">
            <span class="from">{{ result.fromName || result.fromAddress }}</span>
            <time class="date">{{ formatDate(result.dateUnix) }}</time>
          </span>
          <span class="subject">{{ result.subject ?? "(no subject)" }}</span>
          <span class="snippet">
            <template
              v-for="(segment, index) in parseSnippet(result.snippet)"
              :key="index"
            ><mark
              v-if="segment.match"
              class="match"
            >{{ segment.text }}</mark><template v-else>{{ segment.text }}</template></template>
          </span>
        </button>
      </li>
    </ul>
    <EmptyState
      v-else-if="!searching"
      title="No results"
      detail="No messages match your search."
    />
  </section>
</template>

<style scoped>
.list {
  height: 100%;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
  background: var(--color-bg);
}

.count {
  padding: var(--space-2) var(--space-3);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  border-bottom: 1px solid var(--color-border);
}

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

.snippet {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.match {
  background: var(--color-accent-subtle);
  color: var(--color-text);
  border-radius: var(--radius-sm);
  padding: 0 2px;
}
</style>
