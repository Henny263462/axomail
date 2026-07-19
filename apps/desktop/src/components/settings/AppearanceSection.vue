<script setup lang="ts">
/**
 * The "Appearance" settings section: a theme picker (System / Light / Dark).
 *
 * A registry section: its only prop is the shared {@link SettingsSectionContext}.
 * The chosen preference is owned by the settings dialog, which applies it live
 * (via `useTheme`) for an instant preview and persists it with the rest of the
 * batch-saved settings; this component renders the choice and stages selections
 * through `context.setTheme`.
 */
import type { ThemePreference } from "../../types/mail";
import type { SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

/** The options in display order, each with its user-facing label and hint. */
const options: { value: ThemePreference; label: string; hint: string }[] = [
  { value: "system", label: "System", hint: "Match the operating system." },
  { value: "light", label: "Light", hint: "Always use the light theme." },
  { value: "dark", label: "Dark", hint: "Always use the dark theme." },
];

function select(value: ThemePreference): void {
  props.context.setTheme(value);
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      Appearance
    </legend>
    <p class="hint">
      Choose how axomail looks. "System" follows your operating system's
      light or dark setting.
    </p>
    <label
      v-for="option in options"
      :key="option.value"
      class="option"
    >
      <input
        type="radio"
        name="settings-theme"
        :value="option.value"
        :checked="context.theme === option.value"
        @change="select(option.value)"
      >
      <span class="option-text">
        <span class="option-label">{{ option.label }}</span>
        <span class="option-hint">{{ option.hint }}</span>
      </span>
    </label>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.option {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.option-text {
  display: flex;
  flex-direction: column;
}

.option-label {
  font-weight: var(--weight-medium);
}

.option-hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}
</style>
