<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

/** Ceiling shown in the delay input (mirrors the backend clamp: 30 s). */
const MAX_DELAY_SECONDS = 30;

/** The stored delay presented in whole seconds (0 = immediate). */
const delaySeconds = computed(() =>
  Math.round(props.context.autoMarkReadDelayMs / 1000),
);

function onToggle(event: Event): void {
  props.context.setAutoMarkRead(
    (event.target as HTMLInputElement).checked,
    props.context.autoMarkReadDelayMs,
  );
}

function onDelayInput(event: Event): void {
  const seconds = Number((event.target as HTMLInputElement).value);
  const clamped = Math.min(Math.max(0, seconds || 0), MAX_DELAY_SECONDS);
  props.context.setAutoMarkRead(
    props.context.autoMarkReadEnabled,
    Math.round(clamped * 1000),
  );
}
</script>

<template>
  <div class="section">
    <label class="toggle">
      <input
        id="settings-auto-read"
        type="checkbox"
        :checked="context.autoMarkReadEnabled"
        @change="onToggle"
      >
      <span>{{ t("settings.autoRead.markWhenDisplayed") }}</span>
    </label>
    <div class="field">
      <label for="settings-auto-read-delay">{{ t("settings.autoRead.delayLabel") }}</label>
      <input
        id="settings-auto-read-delay"
        type="number"
        inputmode="numeric"
        min="0"
        :max="MAX_DELAY_SECONDS"
        step="1"
        :value="delaySeconds"
        :disabled="!context.autoMarkReadEnabled"
        @input="onDelayInput"
      >
      <p class="hint">
        {{ t("settings.autoRead.delayHint") }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.field input {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  max-width: 8rem;
}

.field input:focus-visible {
  border-color: var(--color-accent);
}

.field input:disabled {
  opacity: 0.6;
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}
</style>
