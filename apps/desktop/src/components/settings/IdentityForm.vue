<script setup lang="ts">
import { ref } from "vue";

import type {
  ArchiveGranularity,
  ForwardMode,
  IdentityDto,
  IdentityInput,
  ReplyPosition,
  SignaturePlacement,
} from "../../types/mail";

const props = defineProps<{
  /** Account the identity belongs to. */
  accountId: number;
  /** Identity being edited; absent for a new identity. */
  identity?: IdentityDto | null;
}>();

const emit = defineEmits<{
  save: [input: IdentityInput];
  cancel: [];
}>();

const existing = props.identity ?? null;

const email = ref(existing?.email ?? "");
const displayName = ref(existing?.displayName ?? "");
const replyTo = ref(existing?.replyTo ?? "");
const signatureText = ref(existing?.signatureText ?? "");
const signaturePlacement = ref<SignaturePlacement>(
  existing?.signaturePlacement ?? "below_quote",
);
const replyPosition = ref<ReplyPosition>(existing?.replyPosition ?? "below");
const forwardMode = ref<ForwardMode>(existing?.forwardMode ?? "inline");
const archiveGranularity = ref<ArchiveGranularity>(
  existing?.archiveGranularity ?? "flat",
);
// An account keeps exactly one default; editing the current default cannot
// unset it here (a checked, disabled box), only another identity can take over.
const makeDefault = ref(existing?.isDefault ?? false);

const error = ref("");

function onSubmit(): void {
  if (email.value.trim().length === 0) {
    error.value = "An identity needs a From address.";
    return;
  }
  error.value = "";
  emit("save", {
    accountId: props.accountId,
    email: email.value.trim(),
    displayName: displayName.value.trim(),
    replyTo: replyTo.value.trim().length > 0 ? replyTo.value.trim() : null,
    signatureText:
      signatureText.value.trim().length > 0 ? signatureText.value : null,
    signaturePlacement: signaturePlacement.value,
    replyPosition: replyPosition.value,
    forwardMode: forwardMode.value,
    archiveGranularity: archiveGranularity.value,
    makeDefault: makeDefault.value,
  });
}
</script>

<template>
  <form
    class="identity-form"
    @submit.prevent="onSubmit"
  >
    <div class="field">
      <label for="identity-email">From address</label>
      <input
        id="identity-email"
        v-model="email"
        type="email"
        autocomplete="off"
        placeholder="you@example.org"
      >
    </div>
    <div class="field">
      <label for="identity-name">Display name</label>
      <input
        id="identity-name"
        v-model="displayName"
        type="text"
        autocomplete="off"
      >
    </div>
    <div class="field">
      <label for="identity-reply-to">Reply-To (optional)</label>
      <input
        id="identity-reply-to"
        v-model="replyTo"
        type="email"
        autocomplete="off"
      >
    </div>
    <div class="field">
      <label for="identity-signature">Signature</label>
      <textarea
        id="identity-signature"
        v-model="signatureText"
        rows="3"
      />
    </div>

    <div class="prefs">
      <div class="field">
        <label for="identity-sig-placement">Signature placement</label>
        <select
          id="identity-sig-placement"
          v-model="signaturePlacement"
        >
          <option value="below_quote">
            Below quoted text
          </option>
          <option value="above_quote">
            Above quoted text
          </option>
        </select>
      </div>
      <div class="field">
        <label for="identity-reply-position">Reply position</label>
        <select
          id="identity-reply-position"
          v-model="replyPosition"
        >
          <option value="above">
            Above the quote
          </option>
          <option value="below">
            Below the quote
          </option>
        </select>
      </div>
      <div class="field">
        <label for="identity-forward-mode">Forward messages</label>
        <select
          id="identity-forward-mode"
          v-model="forwardMode"
        >
          <option value="inline">
            Inline
          </option>
          <option value="attachment">
            As attachment
          </option>
        </select>
      </div>
      <div class="field">
        <label for="identity-archive">Archive folders</label>
        <select
          id="identity-archive"
          v-model="archiveGranularity"
        >
          <option value="flat">
            Single folder
          </option>
          <option value="yearly">
            Per year
          </option>
          <option value="monthly">
            Per month
          </option>
        </select>
      </div>
    </div>

    <label class="toggle">
      <input
        v-model="makeDefault"
        type="checkbox"
        :disabled="existing?.isDefault === true"
      >
      <span>Default identity for this account</span>
    </label>

    <p
      class="error"
      role="alert"
      aria-live="assertive"
    >
      {{ error }}
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
      >
        {{ existing === null ? "Add identity" : "Save identity" }}
      </button>
    </div>
  </form>
</template>

<style scoped>
.identity-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-bg-subtle);
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

.field input,
.field textarea,
.field select {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.field textarea {
  resize: vertical;
}

.prefs {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-2);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
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
}

.button:hover {
  background: var(--color-bg-hover);
}

.button.primary {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}
</style>
