<script setup lang="ts">
/**
 * The multi-selection action bar shown in the list toolbar when rows are
 * selected: Archive / Move to… / Mark read / Delete, plus Clear. Extracted from
 * {@link ThreadedMessageList} to keep it under the file-size gate. The parent
 * resolves the target ids and performs the action; this only emits intent.
 */
import type { BulkAction } from "../../composables/useMailboxActions";

defineEmits<{
  action: [action: BulkAction];
  clear: [];
}>();
</script>

<template>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'archive')"
  >
    Archive
  </button>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'move')"
  >
    Move to…
  </button>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'markRead')"
  >
    Mark read
  </button>
  <button
    type="button"
    class="bulk danger"
    @click="$emit('action', 'delete')"
  >
    Delete
  </button>
  <button
    type="button"
    class="clear-selection"
    @click="$emit('clear')"
  >
    Clear
  </button>
</template>

<style scoped>
.bulk {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.bulk:hover {
  background: var(--color-bg-hover);
}

.bulk.danger {
  color: var(--color-danger);
}

.clear-selection {
  font-size: var(--text-xs);
  color: var(--color-accent);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.clear-selection:hover {
  background: var(--color-bg-hover);
}
</style>
