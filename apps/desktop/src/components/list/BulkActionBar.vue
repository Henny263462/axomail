<script setup lang="ts">
/**
 * The multi-selection action bar shown in the list toolbar when rows are
 * selected: Archive / Move to… / Mark read / Delete, plus Clear. Extracted from
 * {@link ThreadedMessageList} to keep it under the file-size gate. The parent
 * resolves the target ids and performs the action; this only emits intent.
 */
import { useI18n } from "vue-i18n";

import type { BulkAction } from "../../composables/useMailboxActions";
import AppIcon from "../common/AppIcon.vue";

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
    :title="t('list.bulk.archive')"
    @click="$emit('action', 'archive')"
  >
    <AppIcon
      name="archive"
      :size="15"
    />
    <span class="label">{{ t("list.bulk.archive") }}</span>
  </button>
  <button
    type="button"
    class="bulk"
    :title="t('list.bulk.moveTo')"
    @click="$emit('action', 'move')"
  >
    <AppIcon
      name="folderMove"
      :size="15"
    />
    <span class="label">{{ t("list.bulk.moveTo") }}</span>
  </button>
  <button
    type="button"
    class="bulk"
    :title="t('list.bulk.markRead')"
    @click="$emit('action', 'markRead')"
  >
    <AppIcon
      name="mailOpen"
      :size="15"
    />
    <span class="label">{{ t("list.bulk.markRead") }}</span>
  </button>
  <button
    type="button"
    class="bulk danger"
    :title="t('common.delete')"
    @click="$emit('action', 'delete')"
  >
    <AppIcon
      name="trash"
      :size="15"
    />
    <span class="label">{{ t("common.delete") }}</span>
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.label {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.bulk:hover {
  background: var(--color-bg-hover);
}

.bulk.danger {
  color: var(--color-danger);
}

.clear-selection {
  white-space: nowrap;
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
