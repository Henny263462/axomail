<script setup lang="ts">
/**
 * A compact window-layout picker for the status bar: three toggle buttons
 * (Classic / Vertical / Wide) wired to {@link useLayout}. The choice is a pure
 * UI preference persisted in localStorage; it applies live.
 *
 * Rendered as a `role="radiogroup"` so the active layout is announced and the
 * buttons form one labelled group.
 */
import { useI18n } from "vue-i18n";

import { PANE_LAYOUTS, useLayout, type PaneLayout } from "../../composables/useLayout";

const { t } = useI18n({ useScope: "global" });
const { layout, setLayout } = useLayout();

const labelKey: Record<PaneLayout, string> = {
  classic: "layout.classic",
  vertical: "layout.vertical",
  wide: "layout.wide",
};
</script>

<template>
  <div
    class="layouts"
    role="radiogroup"
    :aria-label="t('layout.label')"
  >
    <button
      v-for="option in PANE_LAYOUTS"
      :key="option"
      type="button"
      class="layout"
      :class="{ active: layout === option }"
      role="radio"
      :aria-checked="layout === option"
      :title="t(labelKey[option])"
      @click="setLayout(option)"
    >
      {{ t(labelKey[option]) }}
    </button>
  </div>
</template>

<style scoped>
.layouts {
  display: inline-flex;
  align-items: center;
  gap: 1px;
}

.layout {
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border);
  transition: background var(--transition-fast);
}

.layout:first-child {
  border-radius: var(--radius-sm) 0 0 var(--radius-sm);
}

.layout:last-child {
  border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
}

.layout:not(:first-child) {
  border-left: none;
}

.layout:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.layout.active {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

@media (prefers-reduced-motion: reduce) {
  .layout {
    transition: none;
  }
}
</style>
