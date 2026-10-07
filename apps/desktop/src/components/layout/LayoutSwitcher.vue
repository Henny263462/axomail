<script setup lang="ts">
import { useI18n } from "vue-i18n";

import {
  PANE_LAYOUTS,
  useLayout,
  withViewTransition,
  type PaneLayout,
} from "../../composables/useLayout";
import AppIcon from "../common/AppIcon.vue";
import type { IconName } from "../common/icons";

const { t } = useI18n({ useScope: "global" });
const { layout, setLayout, isDefaultOrder, resetPaneOrder } = useLayout();

const labelKey: Record<PaneLayout, string> = {
  classic: "layout.classic",
  vertical: "layout.vertical",
  wide: "layout.wide",
  focus: "views.focus",
};

const iconFor: Record<PaneLayout, IconName> = {
  classic: "layoutClassic",
  vertical: "layoutVertical",
  wide: "layoutWide",
  focus: "layoutFocus",
};

function choose(option: PaneLayout): void {
  if (option !== layout.value) withViewTransition(() => setLayout(option));
}

function onArrow(event: KeyboardEvent, current: number): void {
  const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
  const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
  if (!forward && !back) return;
  event.preventDefault();
  const count = PANE_LAYOUTS.length;
  const next = (current + (forward ? 1 : -1) + count) % count;
  const option = PANE_LAYOUTS[next];
  if (option === undefined) return;
  choose(option);
  const group = (event.currentTarget as HTMLElement).parentElement;
  (group?.children[next] as HTMLElement | undefined)?.focus();
}
</script>

<template>
  <div class="switcher">
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
        :aria-label="t(labelKey[option])"
        :tabindex="layout === option ? 0 : -1"
        :title="t(labelKey[option])"
        @click="choose(option)"
        @keydown="onArrow($event, index)"
      >
        <AppIcon
          :name="iconFor[option]"
          :size="15"
        />
      </button>
    </div>
    <Transition name="pane-fade">
      <button
        v-if="!isDefaultOrder"
        type="button"
        class="reset"
        :title="t('views.resetOrder')"
        :aria-label="t('views.resetOrder')"
        @click="withViewTransition(resetPaneOrder)"
      >
        <AppIcon
          name="reset"
          :size="14"
        />
      </button>
    </Transition>
  </div>
</template>

<style scoped>
.switcher {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.layouts {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  background: var(--color-bg-sunken);
}

.layout,
.reset {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 20px;
  border-radius: 4px;
  color: var(--color-text-tertiary);
}

.layout:hover,
.reset:hover {
  color: var(--color-text);
}

.layout.active {
  background: var(--color-bg-elevated);
  color: var(--color-accent);
  box-shadow: var(--shadow-sm);
}
</style>
