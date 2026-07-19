<script setup lang="ts">
/**
 * Typed-confirmation dialog for irreversible account deletion.
 *
 * Deleting an account is destructive and local-only: it removes the account
 * and everything axomail stored for it (mail, folders, drafts, and the saved
 * password), but never touches mail still on the server. To make the action
 * deliberate, the delete button stays disabled until the user retypes the
 * account's exact email address. It uses `role="alertdialog"` because it
 * interrupts the flow to confirm a consequential, unrecoverable action.
 */
import { computed, onMounted, ref } from "vue";

const props = defineProps<{
  /** Display name of the account being deleted (shown for orientation). */
  displayName: string;
  /** Email the user must retype to unlock the delete button. */
  email: string;
  /** True while the parent's delete call is in flight. */
  deleting: boolean;
  /** Backend error from a failed delete, shown in the alert region. */
  errorMessage: string;
}>();

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const typed = ref("");
const input = ref<HTMLInputElement | null>(null);

/** True once the typed text matches the account email exactly. */
const confirmed = computed(() => typed.value === props.email);

onMounted(() => {
  input.value?.focus();
});

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    // Keep Escape from also reaching the settings dialog behind this one.
    event.preventDefault();
    event.stopPropagation();
    emit("cancel");
  }
}

function onConfirm(): void {
  if (!confirmed.value || props.deleting) return;
  emit("confirm");
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="delete-account-title"
      aria-describedby="delete-account-body"
    >
      <h3
        id="delete-account-title"
        class="title"
      >
        Delete “{{ displayName }}”?
      </h3>

      <p
        id="delete-account-body"
        class="body"
      >
        This removes the account and everything axomail stored for it on this
        computer — downloaded mail, folders, drafts, and the saved password.
        Mail already on the server is not affected. This cannot be undone.
      </p>

      <label
        class="field"
        for="delete-account-confirm"
      >
        Type <span class="email">{{ email }}</span> to confirm
      </label>
      <input
        id="delete-account-confirm"
        ref="input"
        v-model="typed"
        type="text"
        autocomplete="off"
        spellcheck="false"
        aria-describedby="delete-account-body"
        @keydown.enter.prevent="onConfirm"
      >

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
          type="button"
          class="button danger"
          :disabled="!confirmed || deleting"
          @click="onConfirm"
        >
          {{ deleting ? "Deleting…" : "Delete account" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 110;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 420px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
}

.body {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.field {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.email {
  font-weight: var(--weight-semibold);
  color: var(--color-text);
}

input {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

input:focus-visible {
  border-color: var(--color-accent);
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

.button.danger:not(:disabled) {
  color: var(--color-text-inverse);
  background: var(--color-danger);
  border-color: var(--color-danger);
}
</style>
