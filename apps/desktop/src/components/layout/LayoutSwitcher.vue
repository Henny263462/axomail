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

/**
 * Roving arrow-key navigation for the radio group: Left/Up and Right/Down move
 * the selection (and focus) to the adjacent option, wrapping, as ARIA
 * `radiogroup` semantics require. Selection follows focus, the standard radio
 * behaviour.
 */
function onArrow(event: KeyboardEvent, current: number): void {
  const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
  const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
  if (!forward && !back) return;
  event.preventDefault();
  const count = PANE_LAYOUTS.length;
  const next = (current + (forward ? 1 : -1) + count) % count;
  const option = PANE_LAYOUTS[next];
  if (option === undefined) return;
  setLayout(option);
  const group = (event.currentTarget as HTMLElement).parentElement;
  (group?.children[next] as HTMLElement | undefined)?.focus();
}
</script>

<template>
  <div
    class="layouts"
    role="radiogroup"
    :aria-label="t('layout.label')"
  >
    <button
      v-for="(option, index) in PANE_LAYOUTS"
      :key="option"
      type="button"
      class="layout"
      :class="{ active: layout === option }"
      role="radio"
      :aria-checked="layout === option"
      :tabindex="layout === option ? 0 : -1"
      :title="t(labelKey[option])"
      @click="setLayout(option)"
      @keydown="onArrow($event, index)"
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
