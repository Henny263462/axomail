<script setup lang="ts">
/**
 * The multi-selection action bar shown in the list toolbar when rows are
 * selected: Archive / Move to… / Mark read / Delete, plus Clear. Extracted from
 * {@link ThreadedMessageList} to keep it under the file-size gate. The parent
 * resolves the target ids and performs the action; this only emits intent.
 */
import { useI18n } from "vue-i18n";

import type { BulkAction } from "../../composables/useMailboxActions";

defineEmits<{
  action: [action: BulkAction];
  clear: [];
}>();

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'archive')"
  >
    {{ t("list.bulk.archive") }}
  </button>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'move')"
  >
    {{ t("list.bulk.moveTo") }}
  </button>
  <button
    type="button"
    class="bulk"
    @click="$emit('action', 'markRead')"
  >
    {{ t("list.bulk.markRead") }}
  </button>
  <button
    type="button"
    class="bulk danger"
    @click="$emit('action', 'delete')"
  >
    {{ t("common.delete") }}
  </button>
  <button
    type="button"
    class="clear-selection"
    @click="$emit('clear')"
  >
    {{ t("list.bulk.clear") }}
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
