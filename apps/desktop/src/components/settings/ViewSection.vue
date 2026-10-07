<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useFolderView } from "../../composables/useFolderView";
import { useLayout } from "../../composables/useLayout";
import LayoutSwitcher from "../layout/LayoutSwitcher.vue";
import SidebarViewSwitcher from "../sidebar/SidebarViewSwitcher.vue";
import type { SettingsSectionContext } from "./context";

defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });
const { mode, setMode } = useFolderView();
const { isDefaultOrder, resetPaneOrder } = useLayout();
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("settings.view.title") }}
    </legend>
    <div class="row">
      <span class="row-text">
        <span class="row-label">{{ t("settings.view.layout") }}</span>
        <span class="row-hint">{{ t("settings.view.layoutHint") }}</span>
      </span>
      <LayoutSwitcher />
    </div>
    <div class="row">
      <span class="row-text">
        <span class="row-label">{{ t("settings.view.paneOrder") }}</span>
        <span class="row-hint">{{ t("settings.view.paneOrderHint") }}</span>
      </span>
      <button
        type="button"
        class="small-button"
        :disabled="isDefaultOrder"
        @click="resetPaneOrder"
      >
        {{ t("views.resetOrder") }}
      </button>
    </div>
    <div class="stack">
      <span class="row-text">
        <span class="row-label">{{ t("settings.view.folders") }}</span>
        <span class="row-hint">{{ t("settings.view.foldersHint") }}</span>
      </span>
      <SidebarViewSwitcher
        :mode="mode"
        @select="setMode"
      />
    </div>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.stack {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.row-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.row-label {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
}

.row-hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.small-button {
  flex-shrink: 0;
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
}

.small-button:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.small-button:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}
</style>
