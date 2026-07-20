<script setup lang="ts">
/**
 * The list-pane view for an advanced or saved search: a title header naming the
 * search, above the shared {@link SearchResultsPane} (reused verbatim so a hit
 * looks and behaves exactly like a full-text-search hit). Selecting a hit emits
 * a plain message id, so the host opens the detail as it does elsewhere.
 */
import type { SearchResultDto } from "../../types/mail";
import SearchResultsPane from "../list/SearchResultsPane.vue";

defineProps<{
  /** The search's name / title shown in the header. */
  title: string;
  results: SearchResultDto[];
  /** True while a saved-search run is in flight. */
  searching: boolean;
  selectedMessageId: number | null;
}>();

const emit = defineEmits<{
  select: [messageId: number];
}>();
</script>

<template>
  <section
    class="pane"
    :aria-label="title"
  >
    <header class="head">
      <h2 class="name">
        {{ title }}
      </h2>
    </header>
    <SearchResultsPane
      :results="results"
      :searching="searching"
      :selected-message-id="selectedMessageId"
      @select="emit('select', $event)"
    />
  </section>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.head {
  flex: none;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
}

.name {
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  color: var(--color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pane > :last-child {
  flex: 1;
  min-height: 0;
}
</style>
