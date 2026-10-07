<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useOptionalCommandRegistry } from "../../composables/useCommandRegistry";
import { useLayout, withViewTransition } from "../../composables/useLayout";
import { usePaneDrag } from "../../composables/usePaneDrag";
import { usePaneNavigation, type Pane } from "../../composables/usePaneNavigation";
import { usePaneSizes, type PaneDimension } from "../../composables/usePaneSizes";
import AppIcon from "../common/AppIcon.vue";
import type { IconName } from "../common/icons";
import MainToolbar from "./MainToolbar.vue";
import PaneSplitter from "./PaneSplitter.vue";
import { shellGrid } from "./shellGrid";
import StatusBar from "./StatusBar.vue";

withDefaults(
  defineProps<{
    statusError?: string | null;
    syncing?: boolean;
    syncCount?: number;
  }>(),
  { statusError: null, syncing: false, syncCount: 0 },
);

const emit = defineEmits<{
  dismissStatusError: [];
  openSettings: [];
  openActivity: [];
  cancelSync: [];
}>();

const { t } = useI18n({ useScope: "global" });
const { isStacked, activePane, showPane, paneVisible } = usePaneNavigation();
const { layout, paneOrder, cycleLayout, swapPanes, movePaneBy, resetPaneOrder, toggleFocusLayout } =
  useLayout();
const { sizes, bounds, cssVars, setPaneSize } = usePaneSizes();

const panes: { id: Pane; labelKey: string; icon: IconName }[] = [
  { id: "sidebar", labelKey: "layout.shell.folders", icon: "folder" },
  { id: "list", labelKey: "layout.shell.messages", icon: "inbox" },
  { id: "message", labelKey: "layout.shell.reading", icon: "mail" },
];

const grid = computed(() => shellGrid(layout.value, paneOrder.value));

const columnStyle = computed(() =>
  isStacked.value ? cssVars.value : { ...cssVars.value, gridTemplate: grid.value.template },
);

const drag = usePaneDrag((from, to) => withViewTransition(() => swapPanes(from, to)));

function paneLabel(pane: Pane | null): string {
  const entry = panes.find((candidate) => candidate.id === pane);
  return entry ? t(entry.labelKey) : "";
}

function shown(pane: Pane): boolean {
  if (!paneVisible.value(pane)) return false;
  return isStacked.value || layout.value !== "focus" || pane !== "sidebar";
}

function onResize(dimension: PaneDimension, value: number): void {
  setPaneSize(dimension, value);
}

function onGripKey(event: KeyboardEvent, pane: Pane): void {
  const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
  const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
  if (!back && !forward) return;
  event.preventDefault();
  withViewTransition(() => movePaneBy(pane, back ? -1 : 1));
}

const registry = useOptionalCommandRegistry();
registry?.register({
  id: "view.cycleLayout",
  title: t("layout.cycle"),
  keywords: ["layout", "view", "panes", "classic", "vertical", "wide", "focus"],
  run: () => withViewTransition(cycleLayout),
});
registry?.register({
  id: "view.toggleFolderPane",
  title: t("views.toggleFolders"),
  keywords: ["sidebar", "folders", "focus", "hide", "show"],
  run: () => withViewTransition(toggleFocusLayout),
});
registry?.register({
  id: "view.resetPaneOrder",
  title: t("views.resetOrder"),
  keywords: ["panes", "order", "reset", "move"],
  run: () => withViewTransition(resetPaneOrder),
});

defineExpose({ showPane });
</script>

<template>
  <div
    class="shell"
    :class="{ 'is-dragging': drag.dragging.value !== null }"
  >
    <MainToolbar />
    <nav
      v-if="isStacked"
      class="stack-nav"
      :aria-label="t('layout.shell.panes')"
    >
      <button
        v-for="pane in panes"
        :key="pane.id"
        type="button"
        class="stack-tab"
        :class="{ active: activePane === pane.id }"
        :aria-pressed="activePane === pane.id"
        @click="showPane(pane.id)"
      >
        <AppIcon :name="pane.icon" />
        {{ t(pane.labelKey) }}
      </button>
    </nav>
    <div
      class="columns"
      :class="[isStacked ? 'stacked' : layout]"
      :style="columnStyle"
    >
      <div
        v-for="pane in panes"
        v-show="shown(pane.id)"
        :key="pane.id"
        class="col"
        :class="[
          `col-${pane.id}`,
          { lifted: drag.dragging.value === pane.id, target: drag.over.value === pane.id },
        ]"
        :data-pane="pane.id"
      >
        <slot :name="pane.id" />
        <button
          v-if="!isStacked"
          type="button"
          class="pane-grip"
          :aria-label="t('views.movePane', { pane: t(pane.labelKey) })"
          :title="t('views.movePaneHint')"
          @pointerdown="drag.start($event, pane.id)"
          @keydown="onGripKey($event, pane.id)"
        >
          <AppIcon
            name="grip"
            :size="14"
          />
        </button>
        <div
          v-if="drag.over.value === pane.id"
          class="drop-hint"
          aria-hidden="true"
        >
          <span class="drop-chip">
            <AppIcon name="swap" />
            {{ t("views.dropHere", { pane: paneLabel(drag.dragging.value) }) }}
          </span>
        </div>
      </div>

      <template v-if="!isStacked">
        <PaneSplitter
          v-for="split in grid.splitters"
          :key="`${layout}-${split.area}`"
          class="split"
          :style="{ gridArea: split.area }"
          :orientation="split.orientation"
          :reverse="split.reverse"
          :value="sizes[split.dimension]"
          :min="bounds[split.dimension].min"
          :max="bounds[split.dimension].max"
          :label="t(split.dimension === 'sidebarWidth' ? 'panes.resizeSidebar' : 'panes.resizeList')"
          @resize="onResize(split.dimension, $event)"
        />
      </template>
    </div>
    <div
      v-if="drag.dragging.value !== null"
      class="drag-ghost"
      :style="{ transform: `translate(${drag.pointer.x + 14}px, ${drag.pointer.y + 14}px)` }"
      aria-hidden="true"
    >
      <AppIcon name="grip" />
      {{ paneLabel(drag.dragging.value) }}
    </div>
    <StatusBar
      :error="statusError"
      :syncing="syncing"
      :sync-count="syncCount"
      @dismiss-error="emit('dismissStatusError')"
      @open-settings="emit('openSettings')"
      @open-activity="emit('openActivity')"
      @cancel-sync="emit('cancelSync')"
    />
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--color-bg-subtle);
}

.shell.is-dragging {
  cursor: grabbing;
  user-select: none;
}

.stack-nav {
  display: flex;
  gap: var(--space-1);
  padding: var(--space-2);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
}

.stack-tab {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.stack-tab:hover {
  background: var(--color-bg-hover);
}

.stack-tab.active {
  background: var(--color-bg-elevated);
  color: var(--color-text);
  font-weight: var(--weight-medium);
  box-shadow: var(--shadow-sm);
}

.columns {
  display: grid;
  flex: 1;
  min-height: 0;
  --split: 6px;
}

.col {
  position: relative;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  transition:
    opacity var(--duration-base) ease,
    transform var(--duration-base) var(--ease-out);
}

.col-sidebar {
  grid-area: sidebar;
  view-transition-name: pane-sidebar;
}

.col-list {
  grid-area: list;
  view-transition-name: pane-list;
}

.col-message {
  grid-area: message;
  view-transition-name: pane-message;
}

.columns.stacked {
  grid-template-columns: 1fr;
  grid-template-rows: 1fr;
}

.columns.stacked .col {
  grid-area: 1 / 1;
  animation: ax-rise-in var(--duration-base) var(--ease-out);
}

.col.lifted {
  opacity: 0.55;
  transform: scale(0.985);
}

.pane-grip {
  position: absolute;
  top: 0;
  left: 50%;
  z-index: 5;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 12px;
  translate: -50% 0;
  border-radius: 0 0 8px 8px;
  color: var(--color-text-tertiary);
  background: var(--color-bg-elevated);
  box-shadow: var(--shadow-sm);
  border: 1px solid var(--color-border);
  border-top: none;
  opacity: 0;
  cursor: grab;
  touch-action: none;
}

.pane-grip :deep(.icon) {
  rotate: 90deg;
}

.col:hover > .pane-grip,
.pane-grip:focus-visible {
  opacity: 1;
}

.pane-grip:hover {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.drop-hint {
  position: absolute;
  inset: 6px;
  z-index: 6;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 2px dashed var(--color-accent);
  border-radius: var(--radius-lg);
  background: var(--color-accent-soft);
  animation: ax-fade-in var(--duration-fast) ease;
  pointer-events: none;
}

.drop-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: 999px;
  background: var(--color-accent);
  color: var(--color-text-inverse);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  box-shadow: var(--shadow-md);
  animation: ax-dialog-in var(--duration-base) var(--ease-spring);
}

.drag-ghost {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 300;
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  border-radius: 999px;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-strong);
  box-shadow: var(--shadow-md);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  pointer-events: none;
}
</style>
