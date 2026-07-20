<script setup lang="ts">
/**
 * The "Language" settings section: a UI-language picker (System default /
 * English / German).
 *
 * An autonomous registry section (like Push): its only prop is the shared
 * {@link SettingsSectionContext}. It loads the persisted preference on mount
 * and saves immediately on change through its own `set_language` command, so
 * it does not widen the dialog's batch-saved surface. On save it also applies
 * the locale live via the i18n layer; on failure it reverts both the live
 * locale and the selection and announces the error.
 *
 * It is also the worked example of the Wave 5.4 i18n recipe: its own chrome is
 * rendered through `t()` from `useI18n({ useScope: "global" })`.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { getLanguage, setLanguage } from "../../api/settings";
import { setLanguagePreference, type LanguagePreference } from "../../i18n";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

/** The current preference; loaded on mount, updated optimistically on change. */
const current = ref<LanguagePreference>("system");
const loaded = ref(false);

/** The options in display order, each with its label and hint message keys. */
const options: { value: LanguagePreference; labelKey: string; hintKey: string }[] = [
  { value: "system", labelKey: "language.system", hintKey: "language.systemHint" },
  { value: "en", labelKey: "language.en", hintKey: "language.enHint" },
  { value: "de", labelKey: "language.de", hintKey: "language.deHint" },
];

onMounted(async () => {
  const value = await getLanguage();
  if (value !== null) current.value = value;
  loaded.value = true;
});

/**
 * Applies and persists a language choice. The locale switches optimistically
 * for instant feedback, then persists; a failed persist reverts both the live
 * locale and the selection and announces the error.
 */
async function select(value: LanguagePreference): Promise<void> {
  const previous = current.value;
  if (value === previous) return;
  props.context.reportStatus("");
  if (!isTauri()) {
    props.context.reportError(t("language.statusUnavailable"));
    return;
  }
  current.value = value;
  setLanguagePreference(value);
  try {
    await setLanguage(value);
    props.context.reportError("");
    props.context.reportStatus(t("language.statusSaved"));
  } catch (error) {
    current.value = previous;
    setLanguagePreference(previous);
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("language.title") }}
    </legend>
    <p class="hint">
      {{ t("language.hint") }}
    </p>
    <label
      v-for="option in options"
      :key="option.value"
      class="option"
    >
      <input
        type="radio"
        name="settings-language"
        :value="option.value"
        :checked="current === option.value"
        :disabled="!loaded"
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
