<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import { saveDraft, sendMessage } from "../../api/compose";
import { listIdentities } from "../../api/identities";
import { markReplySource } from "../../api/reply";
import { applySignature, signatureBlock } from "../../lib/signature";
import { useFocusTrap } from "../../composables/useFocusTrap";
import type {
  AccountSummary,
  ComposeAttachmentDto,
  ComposeInput,
  ComposerPrefillDto,
  DraftDto,
  IdentityDto,
  PrefillSourceDto,
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
  /** Reply/forward prefill; when present it seeds the form (see prepare_reply). */
  prefill?: ComposerPrefillDto | null;
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

// Reply/forward context, carried through to the send: the threading headers,
// any attachments (a forwarded message), the source to mark on success, and the
// identity the prefill guessed. `hasPrefill` disables the new-message signature
// handling, since a prefill's body already has the signature placed.
const inReplyTo = ref<string | null>(null);
const references = ref<string[]>([]);
const attachments = ref<ComposeAttachmentDto[]>([]);
const replySource = ref<PrefillSourceDto | null>(null);
const prefillIdentityId = ref<number | null>(null);
const hasPrefill = ref(false);

const sending = ref(false);
const savingDraft = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");

const fields = ref<InstanceType<typeof ComposeFields> | null>(null);
const root = ref<HTMLElement | null>(null);
const { onKeydown } = useFocusTrap(root, close);

/** Title reflecting the mode: editing a draft, replying, forwarding, or new. */
const dialogTitle = computed(() => {
  if (draftId.value !== null) return "Edit draft";
  if (replySource.value !== null) {
    return replySource.value.mark === "forwarded" ? "Forward" : "Reply";
  }
  return "New message";
});

onMounted(async () => {
  const draft = props.draft ?? null;
  const prefill = props.prefill ?? null;
  if (draft !== null) {
    loadDraft(draft);
  } else if (prefill !== null) {
    loadPrefill(prefill);
  }
  const loaded = await listIdentities();
  if (loaded !== null) {
    identities.value = loaded;
    preselectIdentity();
  }
  fields.value?.focus();
});

/** Fills the form from an existing draft. */
function loadDraft(draft: DraftDto): void {
  accountId.value = draft.accountId;
  draftId.value = draft.id;
  to.value = draft.to.join(", ");
  cc.value = draft.cc.join(", ");
  bcc.value = draft.bcc.join(", ");
  subject.value = draft.subject;
  body.value = draft.textBody;
}

/** Fills the form from a reply/forward prefill and stores its send context. */
function loadPrefill(prefill: ComposerPrefillDto): void {
  hasPrefill.value = true;
  accountId.value = prefill.accountId;
  to.value = prefill.to.join(", ");
  cc.value = prefill.cc.join(", ");
  subject.value = prefill.subject;
  body.value = prefill.body;
  inReplyTo.value = prefill.inReplyTo;
  references.value = prefill.references;
  attachments.value = prefill.attachments;
  replySource.value = prefill.source;
  prefillIdentityId.value = prefill.identityId;
}

/** Picks the initial identity — the prefill's guess when present, else the
 * account default — and seeds its signature only for a brand-new blank message
 * (a draft keeps its saved body; a prefill already has the signature placed). */
function preselectIdentity(): void {
  const forAccount = identities.value.filter(
    (identity) => identity.accountId === accountId.value,
  );
  const guessed =
    hasPrefill.value && prefillIdentityId.value !== null
      ? forAccount.find((identity) => identity.id === prefillIdentityId.value)
      : undefined;
  const chosen =
    guessed ?? forAccount.find((identity) => identity.isDefault) ?? forAccount[0];
  if (chosen === undefined) {
    selectedIdentityId.value = null;
    return;
  }
  selectedIdentityId.value = chosen.id;
  if (draftId.value === null && !hasPrefill.value) {
    const block = signatureBlock(chosen.signatureText);
    body.value = applySignature(body.value, "", block);
    currentSignature.value = block;
  }
}

/** Switches the From identity: updates the account and swaps the signature.
 * For a reply/forward the body already carries the placed signature, so the
 * switch changes only the sender (no in-body signature rewrite). */
function onIdentityChange(identityId: number): void {
  const identity = identities.value.find((item) => item.id === identityId);
  if (identity === undefined) return;
  selectedIdentityId.value = identityId;
  accountId.value = identity.accountId;
  if (hasPrefill.value) return;
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
  // Reply/forward context is applied only on the immediate-send path (the
  // backend does not persist it on the outbox row).
  if (inReplyTo.value !== null) {
    input.inReplyTo = inReplyTo.value;
  }
  if (references.value.length > 0) {
    input.references = references.value;
  }
  if (attachments.value.length > 0) {
    input.attachments = attachments.value;
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
    // Only a message that actually left the machine marks its source answered
    // or forwarded (the flag reflects a delivered reply, not a queued one).
    if (outcome.sent) {
      await markSourceSent();
      emit("sent", { draftId: draftId.value });
      return;
    }
    // A scheduled or offline-queued message never leaves now, but it did leave
    // the composer: it waits in the outbox (offline sends flush on reconnect).
    if (
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

/** After a successful reply/forward send, marks the source message answered
 * ($Forwarded for a forward). A no-op for a plain new message. */
async function markSourceSent(): Promise<void> {
  const source = replySource.value;
  if (source === null) return;
  await markReplySource(source.messageId, source.mark);
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
        {{ dialogTitle }}
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
