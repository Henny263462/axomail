<script setup lang="ts">
import type { SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

function onInput(event: Event): void {
  props.context.setSyncIntervalMinutes(
    Number((event.target as HTMLInputElement).value),
  );
}
</script>

<template>
  <div class="field">
    <label for="settings-interval">Sync interval (minutes)</label>
    <input
      id="settings-interval"
      type="number"
      inputmode="numeric"
      min="1"
      step="1"
      :value="context.syncIntervalMinutes"
      @input="onInput"
    >
  </div>
</template>

<style scoped>
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
</style>
