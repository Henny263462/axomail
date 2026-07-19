<script setup lang="ts">
/**
 * The composer's footer: the error/status live regions and the action row
 * (Cancel, Save draft, and the Send / Send-later control).
 *
 * State is owned by the shell and passed down; this component only renders it
 * and emits the user's intent (`cancel`, `saveDraft`, `schedule`). Sending now
 * is the form's own submit, so there is no explicit Send button here — the
 * `SendLaterControl` carries both "Send" and the scheduling menu.
 */
import SendLaterControl from "./SendLaterControl.vue";

defineProps<{
  savingDraft: boolean;
  sending: boolean;
  errorMessage: string;
  statusMessage: string;
}>();

const emit = defineEmits<{
  cancel: [];
  saveDraft: [];
  schedule: [unixSeconds: number];
}>();
</script>

<template>
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
      @click="emit('cancel')"
    >
      Cancel
    </button>
    <button
      type="button"
      class="button"
      :disabled="savingDraft || sending"
      @click="emit('saveDraft')"
    >
      {{ savingDraft ? "Saving…" : "Save draft" }}
    </button>
    <SendLaterControl
      :busy="sending"
      :disabled="savingDraft"
      @schedule="emit('schedule', $event)"
    />
  </div>
</template>

<style scoped>
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
  align-items: flex-start;
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
</style>
