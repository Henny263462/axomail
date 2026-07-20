<script setup lang="ts">
/**
 * Presentational panel for the account-setup connection test: a trigger
 * button plus the per-direction outcome. It owns no logic — the dialog runs
 * the test and passes the result down.
 */
import { useI18n } from "vue-i18n";

import type { ConnectionTestResult } from "../../types/mail";

defineProps<{
  testing: boolean;
  result: ConnectionTestResult | null;
  message: string;
}>();

const emit = defineEmits<{
  test: [];
}>();

const { t } = useI18n({ useScope: "global" });
</script>

<template>
  <div class="test">
    <button
      type="button"
      class="test-connection"
      :disabled="testing"
      @click="emit('test')"
    >
      {{ testing ? t("setup.test.testing") : t("setup.test.action") }}
    </button>
    <div
      v-if="result !== null"
      class="test-results"
      role="status"
      aria-live="polite"
    >
      <p :class="result.incomingOk ? 'ok' : 'fail'">
        {{ t("setup.test.incoming") }}
        {{ result.incomingOk ? t("common.ok") : (result.incomingError ?? t("setup.test.failed")) }}
      </p>
      <p :class="result.outgoingOk ? 'ok' : 'fail'">
        {{ t("setup.test.outgoing") }}
        {{ result.outgoingOk ? t("common.ok") : (result.outgoingError ?? t("setup.test.failed")) }}
      </p>
    </div>
    <p
      v-else-if="message !== ''"
      class="test-message"
      role="status"
      aria-live="polite"
    >
      {{ message }}
    </p>
  </div>
</template>

<style scoped>
.test {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.test-connection {
  align-self: flex-start;
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.test-connection:hover:not(:disabled) {
  background: var(--color-bg-hover);
}

.test-connection:disabled {
  opacity: 0.6;
  cursor: default;
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

.test-message {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}
</style>
