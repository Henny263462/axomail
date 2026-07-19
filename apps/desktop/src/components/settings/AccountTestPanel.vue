<script setup lang="ts">
/**
 * Collapsible connection-test panel for the account edit form.
 *
 * It reuses the same `test_connection` command as account setup. The stored
 * password never leaves the OS keychain, so the test asks the user to retype
 * both passwords; they are used only for this live probe against the
 * edited-but-unsaved server fields and are never saved, logged, or sent to
 * `update_account`. The edited transport fields arrive as reactive props, so
 * the probe always targets whatever is currently in the form.
 */
import { ref } from "vue";

import { testConnection } from "../../api/connection";
import { isTauri } from "../../lib/tauri";
import type {
  AccountDetail,
  ConnectionTestResult,
  ServerEndpointInput,
} from "../../types/mail";

const props = defineProps<{
  account: AccountDetail;
  incoming: ServerEndpointInput;
  outgoing: ServerEndpointInput;
}>();

const open = ref(false);
const testing = ref(false);
const result = ref<ConnectionTestResult | null>(null);
const message = ref("");
const incomingPassword = ref("");
const outgoingPassword = ref("");

async function onTest(): Promise<void> {
  result.value = null;
  if (!isTauri()) {
    message.value =
      "Testing the connection needs the desktop app; it is unavailable in the browser preview.";
    return;
  }
  testing.value = true;
  message.value = "";
  try {
    result.value = await testConnection({
      displayName: props.account.displayName,
      email: props.account.email,
      incoming: {
        protocol: props.account.incoming.protocol,
        host: props.incoming.host.trim(),
        port: props.incoming.port,
        security: props.incoming.security,
        username: props.account.incoming.username,
        password: incomingPassword.value,
      },
      outgoing: {
        host: props.outgoing.host.trim(),
        port: props.outgoing.port,
        security: props.outgoing.security,
        username: props.account.outgoing.username,
        password: outgoingPassword.value,
      },
    });
  } finally {
    testing.value = false;
  }
}
</script>

<template>
  <div class="test">
    <button
      type="button"
      class="link"
      :aria-expanded="open"
      @click="open = !open"
    >
      {{ open ? "Hide connection test" : "Test connection" }}
    </button>
    <div
      v-if="open"
      class="test-body"
    >
      <p class="hint">
        Enter the passwords to probe the edited servers. They are used only for
        this test and are never saved.
      </p>
      <div class="field">
        <label for="edit-incoming-password">Incoming password</label>
        <input
          id="edit-incoming-password"
          v-model="incomingPassword"
          type="password"
          autocomplete="off"
        >
      </div>
      <div class="field">
        <label for="edit-outgoing-password">Outgoing password</label>
        <input
          id="edit-outgoing-password"
          v-model="outgoingPassword"
          type="password"
          autocomplete="off"
        >
      </div>
      <button
        type="button"
        class="button"
        :disabled="testing"
        @click="onTest"
      >
        {{ testing ? "Testing…" : "Run test" }}
      </button>
      <div
        v-if="result !== null"
        class="test-results"
        role="status"
        aria-live="polite"
      >
        <p :class="result.incomingOk ? 'ok' : 'fail'">
          Incoming:
          {{ result.incomingOk ? "OK" : (result.incomingError ?? "Failed") }}
        </p>
        <p :class="result.outgoingOk ? 'ok' : 'fail'">
          Outgoing:
          {{ result.outgoingOk ? "OK" : (result.outgoingError ?? "Failed") }}
        </p>
      </div>
      <p
        v-else-if="message !== ''"
        class="hint"
        role="status"
        aria-live="polite"
      >
        {{ message }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.test {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.test-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
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

.hint {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.test-results {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: var(--text-sm);
}

.test-results .ok {
  color: var(--color-accent);
}

.test-results .fail {
  color: var(--color-danger);
}

.link {
  align-self: flex-start;
  font-size: var(--text-sm);
  color: var(--color-accent);
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
}

.button {
  align-self: flex-start;
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-1) var(--space-3);
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
</style>
