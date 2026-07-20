<script setup lang="ts">
/**
 * Small modal for folder create / rename / delete.
 *
 * In `input` mode it collects a folder name (create or rename). In `delete`
 * mode it confirms removal; when `confirmWord` is set (the folder still holds
 * messages) the Delete button stays disabled until the user retypes that word,
 * so a folder with mail is never dropped by a stray click. Focus is trapped and
 * Escape cancels, via the shared {@link useFocusTrap}.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useFocusTrap } from "../../composables/useFocusTrap";

const props = defineProps<{
  mode: "input" | "delete";
  title: string;
  /** Optional descriptive paragraph. */
  body?: string;
  /** Input label (input mode). */
  label?: string;
  /** Prefilled input value (rename). */
  initialValue?: string;
  /** Confirm button label. */
  confirmText: string;
  /** When set (delete of a non-empty folder), the word the user must retype. */
  confirmWord?: string;
  /** True while the parent's operation is in flight. */
  busy: boolean;
  /** Backend error from a failed operation, shown in the alert region. */
  errorMessage: string;
}>();

const emit = defineEmits<{
  submit: [value: string];
  cancel: [];
}>();

const { t } = useI18n({ useScope: "global" });

const typed = ref(props.mode === "input" ? (props.initialValue ?? "") : "");
const input = ref<HTMLInputElement | null>(null);
const root = ref<HTMLElement | null>(null);
let invoker: HTMLElement | null = null;

/** Whether an input field is shown at all. */
const hasInput = computed(
  () => props.mode === "input" || props.confirmWord !== undefined,
);

/** Whether the confirm button is enabled. */
const confirmed = computed(() => {
  if (props.mode === "input") return typed.value.trim().length > 0;
  if (props.confirmWord !== undefined) return typed.value === props.confirmWord;
  return true;
});

const { onKeydown: trapKeydown } = useFocusTrap(root, () => emit("cancel"));

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") event.stopPropagation();
  trapKeydown(event);
}

onMounted(() => {
  invoker = document.activeElement as HTMLElement | null;
  if (input.value !== null) {
    input.value.focus();
    input.value.select();
  }
});

onBeforeUnmount(() => {
  invoker?.focus();
});

function onConfirm(): void {
  if (!confirmed.value || props.busy) return;
  emit("submit", props.mode === "input" ? typed.value.trim() : "");
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
      :role="mode === 'delete' ? 'alertdialog' : 'dialog'"
      aria-modal="true"
      aria-labelledby="folder-dialog-title"
    >
      <h3
        id="folder-dialog-title"
        class="title"
      >
        {{ title }}
      </h3>

      <p
        v-if="body"
        class="body"
      >
        {{ body }}
      </p>

      <template v-if="hasInput">
        <label
          class="field"
          for="folder-dialog-input"
        >
          <template v-if="mode === 'input'">{{ label ?? t("sidebar.folderDialog.folderName") }}</template>
          <i18n-t
            v-else
            keypath="sidebar.folderDialog.confirmInstruction"
            scope="global"
          >
            <template #word>
              <span class="word">{{ confirmWord }}</span>
            </template>
          </i18n-t>
        </label>
        <input
          id="folder-dialog-input"
          ref="input"
          v-model="typed"
          type="text"
          autocomplete="off"
          spellcheck="false"
          @keydown.enter.prevent="onConfirm"
        >
      </template>

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
          {{ t("common.cancel") }}
        </button>
        <button
          type="button"
          class="button"
          :class="{ danger: mode === 'delete' }"
          :disabled="!confirmed || busy"
          @click="onConfirm"
        >
          {{ busy ? t("sidebar.folderDialog.working") : confirmText }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 380px;
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

.word {
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
