<script setup lang="ts">
/**
 * Edit form for one existing account.
 *
 * Editable: the display name and each server's host, port, and transport
 * security. The incoming protocol and both login names are immutable and are
 * shown read-only for context. Host/port/security live in the two
 * {@link EditServerFields}; the connection test lives in
 * {@link AccountTestPanel}. This component owns the drafts, validation, and
 * the save through `update_account`.
 */
import { computed, ref } from "vue";

import { updateAccount } from "../../api/accounts";
import { isTauri } from "../../lib/tauri";
import type { AccountDetail, ServerEndpointInput } from "../../types/mail";
import AccountTestPanel from "./AccountTestPanel.vue";
import EditServerFields from "./EditServerFields.vue";

const props = defineProps<{
  account: AccountDetail;
}>();

const emit = defineEmits<{
  saved: [];
  cancel: [];
}>();

// Each server draft is a `ref` (not `reactive`), because `EditServerFields`
// updates it by replacing the whole object through `v-model`. A ref keeps
// that reassignment reactive, so the test panel — which receives the draft by
// reference — always probes the current values.
const displayName = ref(props.account.displayName);
const incoming = ref<ServerEndpointInput>({
  host: props.account.incoming.host,
  port: props.account.incoming.port,
  security: props.account.incoming.security,
});
const outgoing = ref<ServerEndpointInput>({
  host: props.account.outgoing.host,
  port: props.account.outgoing.port,
  security: props.account.outgoing.security,
});

const saving = ref(false);
const errorMessage = ref("");

function requiredHost(host: string): string {
  return host.trim() === "" ? "Server is required." : "";
}

function portRange(port: number): string {
  return Number.isInteger(port) && port >= 1 && port <= 65535
    ? ""
    : "Port must be between 1 and 65535.";
}

const displayNameError = computed(() =>
  displayName.value.trim() === "" ? "Display name is required." : "",
);
const incomingHostError = computed(() => requiredHost(incoming.value.host));
const incomingPortError = computed(() => portRange(incoming.value.port));
const outgoingHostError = computed(() => requiredHost(outgoing.value.host));
const outgoingPortError = computed(() => portRange(outgoing.value.port));

const valid = computed(
  () =>
    displayNameError.value === "" &&
    incomingHostError.value === "" &&
    incomingPortError.value === "" &&
    outgoingHostError.value === "" &&
    outgoingPortError.value === "",
);

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not save the account.";
}

async function onSave(): Promise<void> {
  if (!valid.value || saving.value) return;
  if (!isTauri()) {
    errorMessage.value =
      "Saving account changes needs the desktop app; it is unavailable in the browser preview.";
    return;
  }
  saving.value = true;
  try {
    await updateAccount({
      accountId: props.account.id,
      displayName: displayName.value.trim(),
      incoming: {
        host: incoming.value.host.trim(),
        port: incoming.value.port,
        security: incoming.value.security,
      },
      outgoing: {
        host: outgoing.value.host.trim(),
        port: outgoing.value.port,
        security: outgoing.value.security,
      },
    });
    errorMessage.value = "";
    emit("saved");
  } catch (error) {
    errorMessage.value = messageFrom(error);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <form
    class="edit"
    @submit.prevent="onSave"
  >
    <div class="field">
      <label for="edit-display-name">Display name</label>
      <input
        id="edit-display-name"
        v-model="displayName"
        type="text"
        autocomplete="off"
      >
      <p
        v-if="displayNameError"
        class="field-error"
      >
        {{ displayNameError }}
      </p>
    </div>

    <p class="readonly">
      {{ account.email }} · {{ account.incoming.protocol.toUpperCase() }}
    </p>

    <EditServerFields
      v-model="incoming"
      id-prefix="edit-incoming"
      legend="Incoming server"
      :host-error="incomingHostError"
      :port-error="incomingPortError"
    />
    <EditServerFields
      v-model="outgoing"
      id-prefix="edit-outgoing"
      legend="Outgoing server"
      :host-error="outgoingHostError"
      :port-error="outgoingPortError"
    />

    <AccountTestPanel
      :account="account"
      :incoming="incoming"
      :outgoing="outgoing"
    />

    <p
      class="error"
      role="alert"
      aria-live="assertive"
    >
      {{ errorMessage }}
    </p>

    <div class="actions">
      <button
        type="button"
        class="button"
        @click="emit('cancel')"
      >
        Cancel
      </button>
      <button
        type="submit"
        class="button primary"
        :disabled="!valid || saving"
      >
        {{ saving ? "Saving…" : "Save changes" }}
      </button>
    </div>
  </form>
</template>

<style scoped>
.edit {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.readonly {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

input {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  min-width: 0;
}

input:focus-visible {
  border-color: var(--color-accent);
}

.field-error {
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

.button:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.primary:not(:disabled) {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}
</style>
