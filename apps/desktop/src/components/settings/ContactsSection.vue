<script setup lang="ts">
/**
 * Contacts settings section: the "collect sent recipients" toggle. Autonomous —
 * it loads and persists through the contact api and reports outcomes via the
 * shared {@link SettingsSectionContext} aria-live regions.
 */
import { onMounted, ref } from "vue";

import { getCollectAddresses, setCollectAddresses } from "../../api/contacts";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

// Defaults to true (the backend default) until the real value loads.
const collect = ref(true);

onMounted(async () => {
  const value = await getCollectAddresses();
  if (value !== null) collect.value = value;
});

async function onToggle(value: boolean): Promise<void> {
  props.context.reportStatus("");
  if (!isTauri()) {
    props.context.reportError(
      "Contact settings need the desktop app; they are unavailable in the browser preview.",
    );
    return;
  }
  const previous = collect.value;
  collect.value = value;
  try {
    await setCollectAddresses(value);
    props.context.reportError("");
    props.context.reportStatus(
      value
        ? "New recipients will be added to your address book."
        : "Recipients will no longer be collected automatically.",
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
      Contacts
    </legend>
    <label class="row">
      <input
        type="checkbox"
        :checked="collect"
        @change="onToggle(($event.target as HTMLInputElement).checked)"
      >
      <span>
        <span class="label">Collect addresses when sending</span>
        <span class="hint">
          Adds each new To, Cc, and Bcc recipient to your address book after a
          message is sent.
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
