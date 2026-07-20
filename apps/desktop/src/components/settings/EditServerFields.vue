<script setup lang="ts">
/**
 * Host / transport-security / port fields for editing one server of an
 * existing account. Unlike the setup dialog's `ServerConfigFields`, it omits
 * the protocol, username, and password: an account edit may change only the
 * transport, so those immutable fields never appear here. Validation errors
 * are computed by the parent (which gates the Save button) and passed in for
 * display.
 */
import { useI18n } from "vue-i18n";

import type { ServerEndpointInput, TransportSecurity } from "../../types/mail";

const { t } = useI18n({ useScope: "global" });

defineProps<{
  /** Prefix for input ids so incoming and outgoing fields stay unique. */
  idPrefix: string;
  /** Accessible legend for the field group. */
  legend: string;
  /** Message shown under the host field, or empty when it is valid. */
  hostError: string;
  /** Message shown under the port field, or empty when it is valid. */
  portError: string;
}>();

const model = defineModel<ServerEndpointInput>({ required: true });

function value(event: Event): string {
  return (event.target as HTMLInputElement).value;
}

function onHost(event: Event): void {
  model.value = { ...model.value, host: value(event) };
}

function onSecurity(event: Event): void {
  const security: TransportSecurity =
    value(event) === "starttls" ? "starttls" : "tls";
  model.value = { ...model.value, security };
}

function onPort(event: Event): void {
  model.value = { ...model.value, port: Number(value(event)) };
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      {{ legend }}
    </legend>

    <div class="field">
      <label :for="`${idPrefix}-host`">{{ t("settings.accounts.server") }}</label>
      <input
        :id="`${idPrefix}-host`"
        type="text"
        autocomplete="off"
        :value="model.host"
        @input="onHost"
      >
      <p
        v-if="hostError"
        class="field-error"
      >
        {{ hostError }}
      </p>
    </div>

    <div class="row">
      <div class="field">
        <label :for="`${idPrefix}-security`">{{ t("settings.accounts.security") }}</label>
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
        <label :for="`${idPrefix}-port`">{{ t("settings.accounts.port") }}</label>
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
    <p
      v-if="portError"
      class="field-error"
    >
      {{ portError }}
    </p>
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

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.row {
  display: flex;
  gap: var(--space-3);
}

.row .field {
  flex: 1;
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

.field-error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
