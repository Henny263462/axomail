<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import type { AccountSummary } from "../../types/mail";
import AuthMethodFields from "./AuthMethodFields.vue";
import ConnectionTestPanel from "./ConnectionTestPanel.vue";
import ServerConfigFields from "./ServerConfigFields.vue";
import { useAccountSetup } from "./useAccountSetup";

const emit = defineEmits<{
  close: [];
  created: [account: AccountSummary];
}>();

const {
  displayName,
  email,
  mechanism,
  provider,
  clientId,
  customAuthUrl,
  customTokenUrl,
  customScopes,
  incoming,
  outgoing,
  pending,
  errorMessage,
  testing,
  testResult,
  testMessage,
  isOAuth,
  onTest,
  onSubmit,
} = useAccountSetup({ onCreated: (account) => emit("created", account) });

const root = ref<HTMLElement | null>(null);
const firstField = ref<HTMLInputElement | null>(null);

const submitLabel = computed(() => {
  if (pending.value) return isOAuth() ? "Signing in…" : "Adding…";
  return isOAuth() ? "Add account & sign in" : "Add account";
});

onMounted(() => firstField.value?.focus());

function close(): void {
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled])',
    ),
  );
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusables();
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="account-setup-title"
    >
      <h2
        id="account-setup-title"
        class="title"
      >
        Add account
      </h2>

      <form
        class="form"
        @submit.prevent="onSubmit"
      >
        <div class="field">
          <label for="account-display-name">Display name</label>
          <input
            id="account-display-name"
            ref="firstField"
            v-model="displayName"
            type="text"
            autocomplete="off"
          >
        </div>
        <div class="field">
          <label for="account-email">Email address</label>
          <input
            id="account-email"
            v-model="email"
            type="email"
            autocomplete="off"
          >
        </div>

        <AuthMethodFields
          v-model:mechanism="mechanism"
          v-model:provider="provider"
          v-model:client-id="clientId"
          v-model:auth-url="customAuthUrl"
          v-model:token-url="customTokenUrl"
          v-model:scopes="customScopes"
        />

        <ServerConfigFields
          v-model="incoming"
          kind="incoming"
          id-prefix="incoming"
          legend="Incoming mail"
          :hide-password="isOAuth()"
        />
        <ServerConfigFields
          v-model="outgoing"
          kind="outgoing"
          id-prefix="outgoing"
          legend="Outgoing mail (SMTP)"
          :hide-password="isOAuth()"
        />

        <p
          class="error"
          role="status"
          aria-live="polite"
        >
          {{ errorMessage }}
        </p>

        <ConnectionTestPanel
          :testing="testing"
          :result="testResult"
          :message="testMessage"
          @test="onTest"
        />

        <div class="actions">
          <button
            type="button"
            class="button"
            @click="close"
          >
            Cancel
          </button>
          <button
            type="submit"
            class="button primary"
            :disabled="pending"
          >
            {{ submitLabel }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 460px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
  margin-bottom: var(--space-4);
}

.form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
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
}

.field input:focus-visible {
  border-color: var(--color-accent);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
  min-height: 1.25rem;
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

.button:hover {
  background: var(--color-bg-hover);
}

.button.primary {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}

.button.primary:hover {
  background: var(--color-accent-hover);
}

.button.primary:disabled {
  opacity: 0.6;
  cursor: default;
}
</style>
