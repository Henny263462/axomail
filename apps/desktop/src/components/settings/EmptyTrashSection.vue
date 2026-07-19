<script setup lang="ts">
/**
 * Per-account "empty Trash on exit" toggle. When on, the app permanently
 * empties that account's Trash (server expunge + local purge) as it closes.
 * Each account's current value is loaded on mount and persisted immediately
 * through its own command; a rejected toggle reverts.
 */
import { onMounted, ref } from "vue";

import { getEmptyTrashOnExit, setEmptyTrashOnExit } from "../../api/folders";
import { isTauri } from "../../lib/tauri";
import { messageFrom, type SettingsSectionContext } from "./context";

const props = defineProps<{ context: SettingsSectionContext }>();

/** Current value per account id (loaded from the backend on mount). */
const enabled = ref<Record<number, boolean>>({});

onMounted(async () => {
  for (const account of props.context.accounts) {
    const value = await getEmptyTrashOnExit(account.id);
    enabled.value[account.id] = value ?? false;
  }
});

async function onToggle(accountId: number, event: Event): Promise<void> {
  const next = (event.target as HTMLInputElement).checked;
  props.context.reportStatus("");
  if (!isTauri()) {
    (event.target as HTMLInputElement).checked = enabled.value[accountId] ?? false;
    props.context.reportError(
      "Changing this setting needs the desktop app; it is unavailable in the browser preview.",
    );
    return;
  }
  try {
    await setEmptyTrashOnExit(accountId, next);
    enabled.value[accountId] = next;
    props.context.reportError("");
  } catch (error) {
    (event.target as HTMLInputElement).checked = enabled.value[accountId] ?? false;
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset
    v-if="context.accounts.length > 0"
    class="group"
  >
    <legend class="legend">
      Empty Trash on exit
    </legend>
    <p class="hint">
      Permanently deletes each selected account's Trash when the app closes.
    </p>
    <label
      v-for="account in context.accounts"
      :key="account.id"
      class="toggle"
    >
      <input
        type="checkbox"
        :checked="enabled[account.id] ?? false"
        @change="onToggle(account.id, $event)"
      >
      <span>{{ account.displayName }}</span>
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

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
</style>
