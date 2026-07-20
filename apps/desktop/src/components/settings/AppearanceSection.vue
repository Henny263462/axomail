<script setup lang="ts">
/**
 * The "Appearance" settings section: a theme picker (System / Light / Dark).
 *
 * A registry section: its only prop is the shared {@link SettingsSectionContext}.
 * The chosen preference is owned by the settings dialog, which applies it live
 * (via `useTheme`) for an instant preview and persists it with the rest of the
 * batch-saved settings; this component renders the choice and stages selections
 * through `context.setTheme`.
 *
 * This is the Wave 5 Stage 0 i18n proof/template: its user-facing strings are
 * rendered through `t()` from `useI18n({ useScope: "global" })`, with the
 * catalogue in `i18n/locales/{en,de}.json` under `settings.appearance.*`. The
 * English text is unchanged, so it renders identically until 5.4 extracts the
 * rest of the UI the same way.
 */
import { useI18n } from "vue-i18n";

import type { ThemePreference } from "../../types/mail";
import type { SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

/** The options in display order, each with its label and hint message keys. */
const options: { value: ThemePreference; labelKey: string; hintKey: string }[] = [
  { value: "system", labelKey: "settings.appearance.system", hintKey: "settings.appearance.systemHint" },
  { value: "light", labelKey: "settings.appearance.light", hintKey: "settings.appearance.lightHint" },
  { value: "dark", labelKey: "settings.appearance.dark", hintKey: "settings.appearance.darkHint" },
];

function select(value: ThemePreference): void {
  props.context.setTheme(value);
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("settings.appearance.title") }}
    </legend>
    <p class="hint">
      {{ t("settings.appearance.hint") }}
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
        <span class="option-label">{{ t(option.labelKey) }}</span>
        <span class="option-hint">{{ t(option.hintKey) }}</span>
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
