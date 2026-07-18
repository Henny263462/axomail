<script setup lang="ts">
import { onMounted, ref } from "vue";

import { createAccount } from "../../api/accounts";
import type { AccountSummary, CreateAccountInput } from "../../types/mail";
import type { ServerDraft } from "./draft";
import ServerConfigFields from "./ServerConfigFields.vue";

const emit = defineEmits<{
  close: [];
  created: [account: AccountSummary];
}>();

const displayName = ref("");
const email = ref("");

const incoming = ref<ServerDraft>({
  protocol: "imap",
  host: "",
  port: 993,
  security: "tls",
  username: "",
  password: "",
});
const outgoing = ref<ServerDraft>({
  host: "",
  port: 465,
  security: "tls",
  username: "",
  password: "",
});

const pending = ref(false);
const errorMessage = ref("");
const root = ref<HTMLElement | null>(null);
const firstField = ref<HTMLInputElement | null>(null);

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

function validate(): string | null {
  if (displayName.value.trim() === "") return "Enter a display name.";
  const trimmedEmail = email.value.trim();
  if (trimmedEmail === "" || !trimmedEmail.includes("@")) {
    return "Enter a valid email address.";
  }
  if (incoming.value.host.trim() === "") {
    return "Enter the incoming server address.";
  }
  if (incoming.value.username.trim() === "") {
    return "Enter the incoming username.";
  }
  if (incoming.value.password === "") return "Enter the incoming password.";
  if (outgoing.value.host.trim() === "") {
    return "Enter the outgoing server address.";
  }
  if (outgoing.value.username.trim() === "") {
    return "Enter the outgoing username.";
  }
  if (outgoing.value.password === "") return "Enter the outgoing password.";
  return null;
}

function buildInput(): CreateAccountInput {
  return {
    displayName: displayName.value.trim(),
    email: email.value.trim(),
    incoming: {
      protocol: incoming.value.protocol ?? "imap",
      host: incoming.value.host.trim(),
      port: incoming.value.port,
      security: incoming.value.security,
      username: incoming.value.username.trim(),
      password: incoming.value.password,
    },
    outgoing: {
      host: outgoing.value.host.trim(),
      port: outgoing.value.port,
      security: outgoing.value.security,
      username: outgoing.value.username.trim(),
      password: outgoing.value.password,
    },
  };
}

function messageFrom(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Could not create the account.";
}

async function onSubmit(): Promise<void> {
  if (pending.value) return;
  const problem = validate();
  if (problem !== null) {
    errorMessage.value = problem;
    return;
  }
  errorMessage.value = "";
  pending.value = true;
  try {
    const account = await createAccount(buildInput());
    if (account === null) {
      errorMessage.value =
        "Account setup needs the desktop app; it is unavailable in the browser preview.";
      return;
    }
    emit("created", account);
  } catch (error) {
    errorMessage.value = messageFrom(error);
  } finally {
    pending.value = false;
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

        <ServerConfigFields
          v-model="incoming"
          kind="incoming"
          id-prefix="incoming"
          legend="Incoming mail"
        />
        <ServerConfigFields
          v-model="outgoing"
          kind="outgoing"
          id-prefix="outgoing"
          legend="Outgoing mail (SMTP)"
        />

        <p
          class="error"
          role="status"
          aria-live="polite"
        >
          {{ errorMessage }}
        </p>

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
            {{ pending ? "Adding…" : "Add account" }}
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
