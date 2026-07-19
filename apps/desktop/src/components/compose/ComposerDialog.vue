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
import ComposeBody from "./ComposeBody.vue";
import ComposeFields from "./ComposeFields.vue";
import ComposerActions from "./ComposerActions.vue";
import FromPicker from "./FromPicker.vue";

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

const fields = ref<InstanceType<typeof ComposeFields> | null>(null);
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
  fields.value?.focus();
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
    // A scheduled or offline-queued message never leaves now, but it did leave
    // the composer: it waits in the outbox (offline sends flush on reconnect).
    if (
      outcome.sent ||
      (outcome.scheduledAtUnix ?? null) !== null ||
      outcome.queuedOffline === true
    ) {
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
        <ComposeFields
          ref="fields"
          v-model:to="to"
          v-model:cc="cc"
          v-model:bcc="bcc"
          v-model:subject="subject"
        />
        <ComposeBody v-model="body" />
        <ComposerActions
          :saving-draft="savingDraft"
          :sending="sending"
          :error-message="errorMessage"
          :status-message="statusMessage"
          @cancel="close"
          @save-draft="onSaveDraft"
          @schedule="onSchedule"
        />
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
</style>
