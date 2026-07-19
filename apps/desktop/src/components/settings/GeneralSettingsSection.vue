<script setup lang="ts">
/**
 * General application settings: new-mail notifications, the background sync
 * interval, and the per-account POP3 server-deletion toggles.
 *
 * Enabling POP3 server deletion goes through the dedicated
 * `set_pop3_delete_policy` command — the user-visible second guard — never
 * through the bulk save. On success the section emits `close` so the hosting
 * dialog can dismiss.
 */
import { onMounted, ref } from "vue";

import {
  getSettings,
  setPop3DeletePolicy,
  updateSettings,
} from "../../api/settings";
import { isTauri } from "../../lib/tauri";
import type {
  AccountSummary,
  Pop3DeletePolicyDto,
  SettingsDto,
} from "../../types/mail";

/** Exact warning shown beside every POP3 server-deletion toggle. */
const DELETE_WARNING =
  "Deletes mail from the server after download. This cannot be undone.";

const props = defineProps<{
  /** Accounts, used to label each POP3 delete-policy toggle. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
}>();

const notificationsEnabled = ref(false);
const syncIntervalMinutes = ref(5);
const policies = ref<Pop3DeletePolicyDto[]>([]);

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const saving = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");

onMounted(async () => {
  const settings = await getSettings();
  if (settings === null) {
    state.value = "unavailable";
    return;
  }
  notificationsEnabled.value = settings.notificationsEnabled;
  syncIntervalMinutes.value = Math.max(1, settings.syncIntervalMinutes);
  policies.value = settings.pop3DeletePolicies.map((policy) => ({ ...policy }));
  state.value = "ready";
});

function accountName(accountId: number): string {
  const account = props.accounts.find((item) => item.id === accountId);
  return account?.displayName ?? `Account ${accountId}`;
}

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not save settings.";
}

function currentSettings(): SettingsDto {
  return {
    notificationsEnabled: notificationsEnabled.value,
    syncIntervalMinutes: Math.max(1, Math.floor(syncIntervalMinutes.value || 1)),
    pop3DeletePolicies: policies.value.map((policy) => ({ ...policy })),
  };
}

/**
 * Flips one account's POP3 delete policy through the dedicated command — the
 * user-visible second guard. On failure the checkbox reverts and the error is
 * announced; local state changes only after the backend accepts it.
 */
async function onTogglePolicy(
  policy: Pop3DeletePolicyDto,
  event: Event,
): Promise<void> {
  const next = (event.target as HTMLInputElement).checked;
  statusMessage.value = "";
  if (!isTauri()) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    errorMessage.value =
      "Changing the delete policy needs the desktop app; it is unavailable in the browser preview.";
    return;
  }
  try {
    await setPop3DeletePolicy(policy.accountId, next);
    policy.enabled = next;
    errorMessage.value = "";
  } catch (error) {
    (event.target as HTMLInputElement).checked = policy.enabled;
    errorMessage.value = messageFrom(error);
  }
}

async function onSave(): Promise<void> {
  if (saving.value || state.value !== "ready") return;
  statusMessage.value = "";
  if (!isTauri()) {
    errorMessage.value =
      "Saving settings needs the desktop app; it is unavailable in the browser preview.";
    return;
  }
  saving.value = true;
  try {
    await updateSettings(currentSettings());
    errorMessage.value = "";
    emit("close");
  } catch (error) {
    errorMessage.value = messageFrom(error);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <p
    v-if="state === 'loading'"
    class="note"
    role="status"
  >
    Loading settings…
  </p>
  <p
    v-else-if="state === 'unavailable'"
    class="note"
    role="status"
  >
    Settings require the desktop app; they are unavailable in the browser
    preview.
  </p>

  <form
    v-else
    class="form"
    @submit.prevent="onSave"
  >
    <label class="toggle">
      <input
        id="settings-notifications"
        v-model="notificationsEnabled"
        type="checkbox"
      >
      <span>Show desktop notifications for new mail</span>
    </label>

    <div class="field">
      <label for="settings-interval">Sync interval (minutes)</label>
      <input
        id="settings-interval"
        v-model.number="syncIntervalMinutes"
        type="number"
        inputmode="numeric"
        min="1"
        step="1"
      >
    </div>

    <fieldset
      v-if="policies.length > 0"
      class="group"
    >
      <legend class="legend">
        POP3 server deletion
      </legend>
      <p class="warning">
        {{ DELETE_WARNING }}
      </p>
      <label
        v-for="policy in policies"
        :key="policy.accountId"
        class="toggle"
      >
        <input
          type="checkbox"
          :checked="policy.enabled"
          @change="onTogglePolicy(policy, $event)"
        >
        <span>{{ accountName(policy.accountId) }}</span>
      </label>
    </fieldset>

    <p
      class="error"
      role="alert"
      aria-live="assertive"
    >
      {{ errorMessage }}
    </p>
    <p
      class="status"
      role="status"
      aria-live="polite"
    >
      {{ statusMessage }}
    </p>

    <div class="actions">
      <button
        type="button"
        class="button"
        @click="emit('close')"
      >
        Cancel
      </button>
      <button
        type="submit"
        class="button primary"
        :disabled="saving"
      >
        {{ saving ? "Saving…" : "Save" }}
      </button>
    </div>
  </form>
</template>

<style scoped>
.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
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

.warning {
  font-size: var(--text-xs);
  color: var(--color-danger);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
}

.error:empty {
  display: none;
}

.status {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.status:empty {
  display: none;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.primary {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}

.button.primary:hover {
  background: var(--color-accent-hover);
}
</style>
