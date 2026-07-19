<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";

import { saveDraft, sendMessage } from "../../api/compose";
import { listIdentities } from "../../api/identities";
import { markReplySource } from "../../api/reply";
import { signatureBlock, withoutSignature } from "../../lib/signature";
import { mentionsAttachmentKeyword, signatureHtml } from "../../lib/htmlBody";
import { useDraftAutosave } from "../../composables/useDraftAutosave";
import { useFocusTrap } from "../../composables/useFocusTrap";
import type {
  AccountSummary,
  AttachmentCandidateDto,
  ComposeAttachmentDto,
  ComposeInput,
  ComposerPrefillDto,
  DraftDto,
  IdentityDto,
  PrefillSourceDto,
} from "../../types/mail";
import ComposeAttachments from "./ComposeAttachments.vue";
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

const draft = props.draft ?? null;
const isDraft = draft !== null;

const to = ref(draft?.to.join(", ") ?? "");
const cc = ref(draft?.cc.join(", ") ?? "");
const bcc = ref(draft?.bcc.join(", ") ?? "");
const subject = ref(draft?.subject ?? "");
// The editor holds the body WITHOUT the signature. A stored draft baked the
// signature into its text, so strip it on load; the chosen identity's signature
// is re-appended per send/save, never duplicated. A reply/forward prefill is the
// exception: its body already has the signature placed by the quote builder, so
// build-time appending is suppressed (see `hasPrefill`).
const bodyText = ref(draft !== null ? withoutSignature(draft.textBody) : "");
const bodyHtml = ref<string | null>(null);
// Picked files, shown as chips; sent as absolute paths the backend reads.
const attachments = ref<AttachmentCandidateDto[]>([]);

// The account the composed message belongs to and, when editing, the draft id.
// A draft carries its own account, overriding the prop.
const accountId = ref(draft?.accountId ?? props.accountId);
const draftId = ref<number | null>(draft?.id ?? null);

const identities = ref<IdentityDto[]>([]);
const selectedIdentityId = ref<number | null>(null);
/** Plain signature text of the chosen identity, or empty when none applies. */
const currentSignatureText = computed(
  () =>
    identities.value.find((identity) => identity.id === selectedIdentityId.value)
      ?.signatureText ?? "",
);

// Reply/forward context carried through to the send: the threading headers, the
// forwarded message (an inline message/rfc822 part, distinct from picked files),
// the source to mark on success, and the identity the prefill guessed.
// `hasPrefill` disables the new-message signature handling, since a prefill's
// body already has the signature placed.
const inReplyTo = ref<string | null>(null);
const references = ref<string[]>([]);
const forwardedAttachments = ref<ComposeAttachmentDto[]>([]);
const replySource = ref<PrefillSourceDto | null>(null);
const prefillIdentityId = ref<number | null>(null);
const hasPrefill = ref(false);

const sending = ref(false);
const savingDraft = ref(false);
const finished = ref(false);
const errorMessage = ref("");
const statusMessage = ref("");
const attachmentWarning = ref("");

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

// Attaching a file resolves the "you mentioned an attachment" reminder.
watch(
  () => attachments.value.length,
  (count) => {
    if (count > 0) attachmentWarning.value = "";
  },
);

onMounted(async () => {
  const prefill = props.prefill ?? null;
  if (draft === null && prefill !== null) {
    loadPrefill(prefill);
  }
  const loaded = await listIdentities();
  if (loaded !== null) {
    identities.value = loaded;
    preselectIdentity();
  }
  fields.value?.focus();
});

useDraftAutosave({
  snapshot: () =>
    JSON.stringify([to.value, cc.value, bcc.value, subject.value, bodyText.value]),
  isEmpty: isComposerEmpty,
  isBusy: () => sending.value || savingDraft.value,
  isFinished: () => finished.value,
  save: autosave,
});

/** Fills the form from a reply/forward prefill and stores its send context. The
 * prefill body already carries the placed signature (built server-side per the
 * identity's reply prefs), so build-time signature appending is suppressed. */
function loadPrefill(prefill: ComposerPrefillDto): void {
  hasPrefill.value = true;
  accountId.value = prefill.accountId;
  to.value = prefill.to.join(", ");
  cc.value = prefill.cc.join(", ");
  subject.value = prefill.subject;
  bodyText.value = prefill.body;
  inReplyTo.value = prefill.inReplyTo;
  references.value = prefill.references;
  forwardedAttachments.value = prefill.attachments;
  replySource.value = prefill.source;
  prefillIdentityId.value = prefill.identityId;
}

/** Picks the initial identity — the prefill's guess when present, else the
 * account default. The signature is applied at build time (never baked into the
 * editor body), so identity selection does not rewrite the body here. */
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
  selectedIdentityId.value = chosen?.id ?? null;
}

/** Switches the From identity, updating the account it sends from. The signature
 * is merged at build time from {@link currentSignatureText}, so no in-body
 * rewrite is needed here. */
function onIdentityChange(identityId: number): void {
  const identity = identities.value.find((item) => item.id === identityId);
  if (identity === undefined) return;
  selectedIdentityId.value = identityId;
  accountId.value = identity.accountId;
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

/** True when nothing worth keeping has been entered. */
function isComposerEmpty(): boolean {
  return (
    [to.value, cc.value, bcc.value, subject.value, bodyText.value].every(
      (value) => value.trim().length === 0,
    ) && attachments.value.length === 0
  );
}

/** Builds the compose payload, merging the identity signature into the body. A
 * reply/forward prefill already carries its signature, so it is not re-appended
 * (`hasPrefill`). Picked files ride `attachments` (paths); a forwarded message
 * rides `forwardedAttachments` (inline base64) — the backend merges both. */
function buildInput(scheduledAtUnix?: number): ComposeInput {
  const signature = hasPrefill.value ? "" : currentSignatureText.value;
  const html =
    bodyHtml.value === null ? null : bodyHtml.value + signatureHtml(signature);
  const input: ComposeInput = {
    accountId: accountId.value,
    to: splitAddresses(to.value),
    cc: splitAddresses(cc.value),
    bcc: splitAddresses(bcc.value),
    subject: subject.value.trim(),
    textBody: bodyText.value + signatureBlock(signature),
    htmlBody: html !== null && html.trim().length > 0 ? html : null,
    attachments: attachments.value.map((item) => item.path),
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
  if (forwardedAttachments.value.length > 0) {
    input.forwardedAttachments = forwardedAttachments.value;
  }
  return input;
}

/** Submits a compose payload, immediately or scheduled. */
async function submit(input: ComposeInput): Promise<void> {
  if (sending.value || savingDraft.value) return;
  statusMessage.value = "";
  if (input.to.length === 0) {
    errorMessage.value = "Add at least one recipient.";
    return;
  }
  // Attachment-keyword reminder: warn once, then honour a repeated send.
  if (attachmentWarning.value === "" && shouldRemindAttachment()) {
    attachmentWarning.value =
      "Your message mentions an attachment, but none is attached. Send again to send anyway.";
    return;
  }
  attachmentWarning.value = "";
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
      finished.value = true;
      emit("sent", { draftId: draftId.value });
      return;
    }
    // A scheduled or offline-queued message never leaves now, but it did leave
    // the composer: it waits in the outbox (offline sends flush on reconnect).
    if (
      (outcome.scheduledAtUnix ?? null) !== null ||
      outcome.queuedOffline === true
    ) {
      finished.value = true;
      emit("sent", { draftId: draftId.value });
      return;
    }
    errorMessage.value = outcome.error ?? "The message could not be sent.";
  } finally {
    sending.value = false;
  }
}

/** After a successful reply/forward send, marks the source message answered
 * ($Forwarded for a forward). A no-op for a plain new message or draft. */
async function markSourceSent(): Promise<void> {
  const source = replySource.value;
  if (source === null) return;
  await markReplySource(source.messageId, source.mark);
}

/** True when the body hints at an attachment but none is attached. */
function shouldRemindAttachment(): boolean {
  return (
    attachments.value.length === 0 &&
    (mentionsAttachmentKeyword(bodyText.value) ||
      mentionsAttachmentKeyword(subject.value))
  );
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

/** Timer-driven save used by {@link useDraftAutosave}; quiet on failure. */
async function autosave(): Promise<void> {
  const id = await saveDraft(buildInput(), draftId.value);
  if (id !== null) {
    draftId.value = id;
    statusMessage.value = "Draft autosaved.";
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
        <ComposeBody
          v-model:text="bodyText"
          v-model:html="bodyHtml"
          :signature-text="currentSignatureText"
          :is-draft="isDraft"
        />
        <ComposeAttachments
          v-model="attachments"
          @error="errorMessage = $event"
        />
        <p
          v-if="attachmentWarning !== ''"
          class="warning"
          role="alert"
        >
          {{ attachmentWarning }}
        </p>
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

.warning {
  font-size: var(--text-sm);
  color: var(--color-warning, var(--color-text));
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-2);
}
</style>
