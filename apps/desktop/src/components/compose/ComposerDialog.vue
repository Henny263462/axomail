<script setup lang="ts">
import { onMounted, ref } from "vue";

import { saveDraft, sendMessage } from "../../api/compose";
import { listIdentities } from "../../api/identities";
import { applySignature, signatureBlock } from "../../lib/signature";
import { useFocusTrap } from "../../composables/useFocusTrap";
import type {
  AccountSummary,
  ComposeInput,
  DraftDto,
  IdentityDto,
} from "../../types/mail";
import FromPicker from "./FromPicker.vue";
import SendLaterControl from "./SendLaterControl.vue";

const props = defineProps<{
  /** Account to compose from when not editing an existing draft. */
  accountId: number;
  /** Draft being edited; when present its fields prefill the form. */
  draft?: DraftDto | null;
  /** Accounts, used only to label the From picker's identity groups. */
  accounts?: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
  sent: [payload: { draftId: number | null }];
  saved: [draftId: number];
}>();

const to = ref("");
const cc = ref("");
const bcc = ref("");
const subject = ref("");
const body = ref("");

// The account the composed message belongs to and, when editing, the draft
// id updated on save. A draft carries its own account, overriding the prop.
const accountId = ref(props.accountId);
const draftId = ref<number | null>(null);

// Sender identities and the chosen one. The signature block currently in the
// body is tracked so switching identity swaps it in place (see signature.ts).
const identities = ref<IdentityDto[]>([]);
const selectedIdentityId = ref<number | null>(null);
const currentSignature = ref("");

const sending = ref(false);
const savingDraft = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");

const firstField = ref<HTMLInputElement | null>(null);
const root = ref<HTMLElement | null>(null);
const { onKeydown } = useFocusTrap(root, close);

onMounted(async () => {
  const draft = props.draft ?? null;
  if (draft !== null) {
    accountId.value = draft.accountId;
    draftId.value = draft.id;
    to.value = draft.to.join(", ");
    cc.value = draft.cc.join(", ");
    bcc.value = draft.bcc.join(", ");
    subject.value = draft.subject;
    body.value = draft.textBody;
  }
  const loaded = await listIdentities();
  if (loaded !== null) {
    identities.value = loaded;
    preselectIdentity();
  }
  firstField.value?.focus();
});

/** Picks the default identity for the current account and seeds its signature
 * (only for a brand-new message; a draft keeps its saved body). */
function preselectIdentity(): void {
  const forAccount = identities.value.filter(
    (identity) => identity.accountId === accountId.value,
  );
  const chosen = forAccount.find((identity) => identity.isDefault) ?? forAccount[0];
  if (chosen === undefined) {
    selectedIdentityId.value = null;
    return;
  }
  selectedIdentityId.value = chosen.id;
  if (draftId.value === null) {
    const block = signatureBlock(chosen.signatureText);
    body.value = applySignature(body.value, "", block);
    currentSignature.value = block;
  }
}

/** Switches the From identity: updates the account and swaps the signature. */
function onIdentityChange(identityId: number): void {
  const identity = identities.value.find((item) => item.id === identityId);
  if (identity === undefined) return;
  selectedIdentityId.value = identityId;
  accountId.value = identity.accountId;
  const block = signatureBlock(identity.signatureText);
  body.value = applySignature(body.value, currentSignature.value, block);
  currentSignature.value = block;
}

function close(): void {
  emit("close");
}

/** Splits a comma-separated recipient field into trimmed, non-empty parts. */
function splitAddresses(raw: string): string[] {
  return raw
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

function buildInput(scheduledAtUnix?: number): ComposeInput {
  const input: ComposeInput = {
    accountId: accountId.value,
    to: splitAddresses(to.value),
    cc: splitAddresses(cc.value),
    bcc: splitAddresses(bcc.value),
    subject: subject.value.trim(),
    textBody: body.value,
  };
  if (selectedIdentityId.value !== null) {
    input.identityId = selectedIdentityId.value;
  }
  if (scheduledAtUnix !== undefined) {
    input.scheduledAtUnix = scheduledAtUnix;
  }
  return input;
}

/** Submits a compose payload, immediately or scheduled. */
async function submit(input: ComposeInput): Promise<void> {
  // Sending and draft-saving are mutually exclusive: a Save racing a
  // successful Send would resurrect the message as an unintended draft.
  if (sending.value || savingDraft.value) return;
  statusMessage.value = "";
  if (input.to.length === 0) {
    errorMessage.value = "Add at least one recipient.";
    return;
  }
  errorMessage.value = "";
  sending.value = true;
  try {
    const outcome = await sendMessage(input);
    if (outcome === null) {
      errorMessage.value =
        "Sending needs the desktop app; it is unavailable in the browser preview.";
      return;
    }
    // A scheduled message never leaves now, but it did leave the composer.
    if (outcome.sent || (outcome.scheduledAtUnix ?? null) !== null) {
      emit("sent", { draftId: draftId.value });
      return;
    }
    errorMessage.value = outcome.error ?? "The message could not be sent.";
  } finally {
    sending.value = false;
  }
}

function onSend(): Promise<void> {
  return submit(buildInput());
}

function onSchedule(unixSeconds: number): Promise<void> {
  return submit(buildInput(unixSeconds));
}

async function onSaveDraft(): Promise<void> {
  if (savingDraft.value || sending.value) return;
  errorMessage.value = "";
  savingDraft.value = true;
  try {
    const id = await saveDraft(buildInput(), draftId.value);
    if (id === null) {
      errorMessage.value =
        "Saving needs the desktop app; it is unavailable in the browser preview.";
      return;
    }
    draftId.value = id;
    statusMessage.value = "Draft saved.";
    emit("saved", id);
  } finally {
    savingDraft.value = false;
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
      aria-labelledby="composer-title"
    >
      <h2
        id="composer-title"
        class="title"
      >
        {{ draftId === null ? "New message" : "Edit draft" }}
      </h2>

      <form
        class="form"
        @submit.prevent="onSend"
      >
        <FromPicker
          v-if="identities.length > 0"
          :identities="identities"
          :model-value="selectedIdentityId"
          :accounts="accounts"
          @update:model-value="onIdentityChange"
        />
        <div class="field">
          <label for="compose-to">To</label>
          <input
            id="compose-to"
            ref="firstField"
            v-model="to"
            type="text"
            autocomplete="off"
            placeholder="Comma-separated addresses"
          >
        </div>
        <div class="field">
          <label for="compose-cc">Cc</label>
          <input
            id="compose-cc"
            v-model="cc"
            type="text"
            autocomplete="off"
          >
        </div>
        <div class="field">
          <label for="compose-bcc">Bcc</label>
          <input
            id="compose-bcc"
            v-model="bcc"
            type="text"
            autocomplete="off"
          >
        </div>
        <div class="field">
          <label for="compose-subject">Subject</label>
          <input
            id="compose-subject"
            v-model="subject"
            type="text"
            autocomplete="off"
          >
        </div>
        <div class="field">
          <label for="compose-body">Message</label>
          <textarea
            id="compose-body"
            v-model="body"
            rows="10"
          />
        </div>

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
            @click="close"
          >
            Cancel
          </button>
          <button
            type="button"
            class="button"
            :disabled="savingDraft || sending"
            @click="onSaveDraft"
          >
            {{ savingDraft ? "Saving…" : "Save draft" }}
          </button>
          <SendLaterControl
            :busy="sending"
            :disabled="savingDraft"
            @schedule="onSchedule"
          />
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
  max-width: 560px;
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
  gap: var(--space-3);
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
.field textarea {
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
  min-height: 8rem;
}

.field input:focus-visible,
.field textarea:focus-visible {
  border-color: var(--color-accent);
}

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
