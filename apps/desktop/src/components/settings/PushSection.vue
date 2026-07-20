<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { getPushEnabled, setPushEnabled } from "../../api/settings";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

// Autonomous setting: loaded on mount and saved immediately on toggle, so it
// does not widen the dialog's batch-saved surface.
const enabled = ref(true);
const loaded = ref(false);

onMounted(async () => {
  const value = await getPushEnabled();
  if (value !== null) enabled.value = value;
  loaded.value = true;
});

/**
 * Flips the push preference through its dedicated command. On failure the
 * checkbox reverts and the error is announced; local state changes only after
 * the backend accepts it.
 */
async function onToggle(event: Event): Promise<void> {
  const next = (event.target as HTMLInputElement).checked;
  props.context.reportStatus("");
  if (!isTauri()) {
    (event.target as HTMLInputElement).checked = enabled.value;
    props.context.reportError(t("settings.push.unavailable"));
    return;
  }
  try {
    await setPushEnabled(next);
    enabled.value = next;
    props.context.reportError("");
    props.context.reportStatus(
      next ? t("settings.push.enabled") : t("settings.push.disabled"),
    );
  } catch (error) {
    (event.target as HTMLInputElement).checked = enabled.value;
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("settings.push.title") }}
    </legend>
    <p class="hint">
      {{ t("settings.push.hint") }}
    </p>
    <label class="toggle">
      <input
        type="checkbox"
        :checked="enabled"
        :disabled="!loaded"
        :aria-label="t('settings.push.toggleAria')"
        @change="onToggle($event)"
      >
      <span>{{ t("settings.push.toggleLabel") }}</span>
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
  color: var(--color-text-tertiary);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
