<script setup lang="ts">
/**
 * Contacts settings section: the "collect sent recipients" toggle. Autonomous —
 * it loads and persists through the contact api and reports outcomes via the
 * shared {@link SettingsSectionContext} aria-live regions.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { getCollectAddresses, setCollectAddresses } from "../../api/contacts";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

const { t } = useI18n({ useScope: "global" });

// Defaults to true (the backend default) until the real value loads.
const collect = ref(true);

onMounted(async () => {
  const value = await getCollectAddresses();
  if (value !== null) collect.value = value;
});

async function onToggle(value: boolean): Promise<void> {
  props.context.reportStatus("");
  if (!isTauri()) {
    props.context.reportError(t("settings.contacts.unavailable"));
    return;
  }
  const previous = collect.value;
  collect.value = value;
  try {
    await setCollectAddresses(value);
    props.context.reportError("");
    props.context.reportStatus(
      value
        ? t("settings.contacts.collectOn")
        : t("settings.contacts.collectOff"),
    );
  } catch (error) {
    collect.value = previous;
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ t("settings.contacts.title") }}
    </legend>
    <label class="row">
      <input
        type="checkbox"
        :checked="collect"
        @change="onToggle(($event.target as HTMLInputElement).checked)"
      >
      <span>
        <span class="label">{{ t("settings.contacts.collectLabel") }}</span>
        <span class="hint">
          {{ t("settings.contacts.collectHint") }}
        </span>
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

.row {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.label {
  display: block;
  color: var(--color-text);
}

.hint {
  display: block;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}
</style>
