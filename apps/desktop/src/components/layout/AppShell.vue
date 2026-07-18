<script setup lang="ts">
import { usePaneNavigation, type Pane } from "../../composables/usePaneNavigation";
import StatusBar from "./StatusBar.vue";

withDefaults(
  defineProps<{
    /** Redacted sync failure surfaced in the status bar, or null. */
    statusError?: string | null;
  }>(),
  { statusError: null },
);

const emit = defineEmits<{
  dismissStatusError: [];
  openSettings: [];
}>();

const { isStacked, activePane, showPane, paneVisible } = usePaneNavigation();

const panes: { id: Pane; label: string }[] = [
  { id: "sidebar", label: "Folders" },
  { id: "list", label: "Messages" },
  { id: "message", label: "Reading" },
];

defineExpose({ showPane });
</script>

<template>
  <div class="shell">
    <nav
      v-if="isStacked"
      class="stack-nav"
      aria-label="Panes"
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
        {{ pane.label }}
      </button>
    </nav>
    <div
      class="columns"
      :class="{ stacked: isStacked }"
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
    </div>
    <StatusBar
      :error="statusError"
      @dismiss-error="emit('dismissStatusError')"
      @open-settings="emit('openSettings')"
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
  grid-template-columns: var(--sidebar-width) var(--list-width) 1fr;
  flex: 1;
  min-height: 0;
}

.columns.stacked {
  grid-template-columns: 1fr;
}

.col {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
</style>
