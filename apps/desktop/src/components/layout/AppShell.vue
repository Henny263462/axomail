<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useOptionalCommandRegistry } from "../../composables/useCommandRegistry";
import { useLayout } from "../../composables/useLayout";
import { usePaneNavigation, type Pane } from "../../composables/usePaneNavigation";
import {
  usePaneSizes,
  type PaneDimension,
} from "../../composables/usePaneSizes";
import PaneSplitter from "./PaneSplitter.vue";
import StatusBar from "./StatusBar.vue";

withDefaults(
  defineProps<{
    /** Redacted sync failure surfaced in the status bar, or null. */
    statusError?: string | null;
    /** True while at least one sync job is queued or running. */
    syncing?: boolean;
    /** Number of jobs currently queued or running. */
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
const { layout, cycleLayout } = useLayout();
const { sizes, bounds, cssVars, setPaneSize } = usePaneSizes();

const panes: { id: Pane; labelKey: string }[] = [
  { id: "sidebar", labelKey: "layout.shell.folders" },
  { id: "list", labelKey: "layout.shell.messages" },
  { id: "message", labelKey: "layout.shell.reading" },
];

function onResize(dimension: PaneDimension, value: number): void {
  setPaneSize(dimension, value);
}

// A palette "View" command to rotate the layout; registered only when the app
// root provides a command registry (a no-op for a standalone unit mount).
const registry = useOptionalCommandRegistry();
registry?.register({
  id: "view.cycleLayout",
  title: t("layout.cycle"),
  keywords: ["layout", "view", "panes", "classic", "vertical", "wide"],
  run: cycleLayout,
});

defineExpose({ showPane });
</script>

<template>
  <div class="shell">
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
        {{ t(pane.labelKey) }}
      </button>
    </nav>
    <div
      class="columns"
      :class="[isStacked ? 'stacked' : layout]"
      :style="cssVars"
    >
      <div
        v-show="paneVisible('sidebar')"
        class="col col-sidebar"
      >
        <slot name="sidebar" />
      </div>
      <div
        v-show="paneVisible('list')"
        class="col col-list"
      >
        <slot name="list" />
      </div>
      <div
        v-show="paneVisible('message')"
        class="col col-message"
      >
        <slot name="message" />
      </div>

      <!-- Splitters: present only on a non-stacked shell (a narrow, one-pane
           viewport has nothing to resize between). Which ones apply depends on
           the layout; each drives one persisted dimension. -->
      <template v-if="!isStacked">
        <PaneSplitter
          class="split split-a"
          orientation="vertical"
          :value="sizes.sidebarWidth"
          :min="bounds.sidebarWidth.min"
          :max="bounds.sidebarWidth.max"
          :label="t('panes.resizeSidebar')"
          @resize="onResize('sidebarWidth', $event)"
        />
        <PaneSplitter
          v-if="layout === 'vertical'"
          class="split split-b"
          orientation="vertical"
          :value="sizes.listWidth"
          :min="bounds.listWidth.min"
          :max="bounds.listWidth.max"
          :label="t('panes.resizeList')"
          @resize="onResize('listWidth', $event)"
        />
        <PaneSplitter
          v-else
          class="split split-c"
          orientation="horizontal"
          :value="sizes.listHeight"
          :min="bounds.listHeight.min"
          :max="bounds.listHeight.max"
          :label="t('panes.resizeList')"
          @resize="onResize('listHeight', $event)"
        />
      </template>
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
}

.stack-nav {
  display: flex;
  gap: var(--space-1);
  padding: var(--space-2);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
}

.stack-tab {
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  transition: background var(--transition-fast);
}

.stack-tab:hover {
  background: var(--color-bg-hover);
}

.stack-tab.active {
  background: var(--color-bg-selected);
  color: var(--color-text);
  font-weight: var(--weight-medium);
}

.columns {
  display: grid;
  flex: 1;
  min-height: 0;
  /* Splitter track thickness. */
  --split: 6px;
}

.col {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

/* Vertical: folder pane, list, and reading pane as three side-by-side columns. */
.columns.vertical {
  grid-template:
    "sidebar splitA list splitB message" 1fr
    / var(--sidebar-width) var(--split) var(--list-width) var(--split) 1fr;
}

/* Classic: folder pane left (full height); list above the reading pane. */
.columns.classic {
  grid-template:
    "sidebar splitA list" var(--list-height)
    "sidebar splitA splitC" var(--split)
    "sidebar splitA message" 1fr
    / var(--sidebar-width) var(--split) 1fr;
}

/* Wide: full-width list across the top; folder + reading panes below. */
.columns.wide {
  grid-template:
    "list list list" var(--list-height)
    "splitC splitC splitC" var(--split)
    "sidebar splitA message" 1fr
    / var(--sidebar-width) var(--split) 1fr;
}

.columns.vertical .col-sidebar,
.columns.classic .col-sidebar,
.columns.wide .col-sidebar {
  grid-area: sidebar;
}

.columns.vertical .col-list,
.columns.classic .col-list,
.columns.wide .col-list {
  grid-area: list;
}

.columns.vertical .col-message,
.columns.classic .col-message,
.columns.wide .col-message {
  grid-area: message;
}

.split-a {
  grid-area: splitA;
}

.split-b {
  grid-area: splitB;
}

.split-c {
  grid-area: splitC;
}

/* Stacked (narrow viewport): one pane at a time; the splitters are absent. */
.columns.stacked {
  grid-template-columns: 1fr;
  grid-template-rows: 1fr;
}
</style>
