<script setup lang="ts">
import { ref } from "vue";

import type { IncomingProtocol, TransportSecurity } from "../../types/mail";
import type { ServerDraft } from "./draft";
import { incomingPort, outgoingPort } from "./ports";

const props = defineProps<{
  /** Section kind; the incoming section additionally shows a protocol select. */
  kind: "incoming" | "outgoing";
  /** Prefix for input ids so incoming and outgoing fields stay unique. */
  idPrefix: string;
  /** Accessible legend for the field group. */
  legend: string;
}>();

const model = defineModel<ServerDraft>({ required: true });

// True once the user edits the port by hand; suppresses port autofill.
const portEdited = ref(false);

function value(event: Event): string {
  return (event.target as HTMLInputElement).value;
}

function suggestedPort(draft: ServerDraft): number {
  if (props.kind === "incoming") {
    return incomingPort(draft.protocol ?? "imap", draft.security);
  }
  return outgoingPort(draft.security);
}

function applyAutofill(next: ServerDraft): ServerDraft {
  if (!portEdited.value) next.port = suggestedPort(next);
  return next;
}

function onProtocol(event: Event): void {
  const protocol: IncomingProtocol = value(event) === "pop3" ? "pop3" : "imap";
  model.value = applyAutofill({ ...model.value, protocol });
}

function onSecurity(event: Event): void {
  const security: TransportSecurity =
    value(event) === "starttls" ? "starttls" : "tls";
  model.value = applyAutofill({ ...model.value, security });
}

function onPort(event: Event): void {
  portEdited.value = true;
  model.value = { ...model.value, port: Number(value(event)) };
}

function onHost(event: Event): void {
  model.value = { ...model.value, host: value(event) };
}

function onUsername(event: Event): void {
  model.value = { ...model.value, username: value(event) };
}

function onPassword(event: Event): void {
  model.value = { ...model.value, password: value(event) };
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ legend }}
    </legend>

    <div
      v-if="kind === 'incoming'"
      class="field"
    >
      <label :for="`${idPrefix}-protocol`">Protocol</label>
      <select
        :id="`${idPrefix}-protocol`"
        :value="model.protocol ?? 'imap'"
        @change="onProtocol"
      >
        <option value="imap">
          IMAP
        </option>
        <option value="pop3">
          POP3
        </option>
      </select>
    </div>

    <div class="field">
      <label :for="`${idPrefix}-host`">Server</label>
      <input
        :id="`${idPrefix}-host`"
        type="text"
        autocomplete="off"
        :value="model.host"
        @input="onHost"
      >
    </div>

    <div class="row">
      <div class="field">
        <label :for="`${idPrefix}-security`">Security</label>
        <select
          :id="`${idPrefix}-security`"
          :value="model.security"
          @change="onSecurity"
        >
          <option value="tls">
            TLS
          </option>
          <option value="starttls">
            STARTTLS
          </option>
        </select>
      </div>
      <div class="field">
        <label :for="`${idPrefix}-port`">Port</label>
        <input
          :id="`${idPrefix}-port`"
          type="number"
          inputmode="numeric"
          min="1"
          max="65535"
          :value="model.port"
          @input="onPort"
        >
      </div>
    </div>

    <div class="field">
      <label :for="`${idPrefix}-username`">Username</label>
      <input
        :id="`${idPrefix}-username`"
        type="text"
        autocomplete="off"
        :value="model.username"
        @input="onUsername"
      >
    </div>

    <div class="field">
      <label :for="`${idPrefix}-password`">Password</label>
      <input
        :id="`${idPrefix}-password`"
        type="password"
        autocomplete="off"
        :value="model.password"
        @input="onPassword"
      >
    </div>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.row {
  display: flex;
  gap: var(--space-3);
}

.row .field {
  flex: 1;
}

label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

input,
select {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  min-width: 0;
}

input:focus-visible,
select:focus-visible {
  border-color: var(--color-accent);
}
</style>
