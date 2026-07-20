<script setup lang="ts">
/**
 * The sidebar folder-view mode switcher: All / Unified / Favorite / Tags /
 * Recent. A `role="tablist"`-style segmented control (each button a `radio`) so
 * the active mode is announced. The chosen mode is owned by {@link useFolderView}
 * (persisted); this component only renders the current value and emits changes.
 */
import { useI18n } from "vue-i18n";

import {
  FOLDER_VIEW_MODES,
  type FolderViewMode,
} from "../../composables/useFolderView";

defineProps<{ mode: FolderViewMode }>();

const emit = defineEmits<{ select: [mode: FolderViewMode] }>();

const { t } = useI18n({ useScope: "global" });

const labelKey: Record<FolderViewMode, string> = {
  all: "folderView.all",
  unified: "folderView.unified",
  favorite: "folderView.favorite",
  tags: "folderView.tags",
  recent: "folderView.recent",
};

/**
 * Roving arrow-key navigation for the radio group: Left/Up and Right/Down move
 * the selection (and focus) to the adjacent mode, wrapping, as ARIA
 * `radiogroup` semantics require. Selection follows focus.
 */
function onArrow(event: KeyboardEvent, current: number): void {
  const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
  const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
  if (!forward && !back) return;
  event.preventDefault();
  const count = FOLDER_VIEW_MODES.length;
  const next = (current + (forward ? 1 : -1) + count) % count;
  const option = FOLDER_VIEW_MODES[next];
  if (option === undefined) return;
  emit("select", option);
  const group = (event.currentTarget as HTMLElement).parentElement;
  (group?.children[next] as HTMLElement | undefined)?.focus();
}
</script>

<template>
  <div
    class="switcher"
    role="radiogroup"
    :aria-label="t('folderView.label')"
  >
    <button
      v-for="(option, index) in FOLDER_VIEW_MODES"
      :key="option"
      type="button"
      class="mode"
      :class="{ active: mode === option }"
      role="radio"
      :aria-checked="mode === option"
      :tabindex="mode === option ? 0 : -1"
      @click="emit('select', option)"
      @keydown="onArrow($event, index)"
    >
      {{ t(labelKey[option]) }}
    </button>
  </div>
</template>

<style scoped>
.switcher {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
  margin-bottom: var(--space-2);
}

.mode {
  flex: 1 1 auto;
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.mode:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.mode.active {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

@media (prefers-reduced-motion: reduce) {
  .mode {
    transition: none;
  }
}
</style>
