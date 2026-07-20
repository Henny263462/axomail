<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";

import { listIdentities } from "../../api/identities";
import { withoutSignature } from "../../lib/signature";
import { useComposerSubmit } from "../../composables/useComposerSubmit";
import { useDraftAutosave } from "../../composables/useDraftAutosave";
import { useFocusTrap } from "../../composables/useFocusTrap";
import type {
  AccountSummary,
  AttachmentCandidateDto,
  ComposeAttachmentDto,
  ComposeInitial,
  ComposerPrefillDto,
  DraftDto,
  IdentityDto,
  PrefillSourceDto,
} from "../../types/mail";
import ComposeAttachments from "./ComposeAttachments.vue";
import ComposeBody from "./ComposeBody.vue";
import ComposeFields from "./ComposeFields.vue";
import ComposePgpControls from "./ComposePgpControls.vue";
import ComposerActions from "./ComposerActions.vue";
import FromPicker from "./FromPicker.vue";

const props = defineProps<{
  /** Account to compose from when not editing an existing draft. */
  accountId: number;
  /** Draft being edited; when present its fields prefill the form. */
  draft?: DraftDto | null;
  /** Reply/forward prefill; when present it seeds the form (see prepare_reply). */
  prefill?: ComposerPrefillDto | null;
  /** New-message-from-template seed; fills a fresh composition (no draft id). */
  initial?: ComposeInitial | null;
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

// OpenPGP protection for this message (Track 5.1). Availability is decided by
// `ComposePgpControls` against the sender and recipients; the passphrase (for a
// protected signing key) rides only the send, never persisted.
const sign = ref(false);
const encrypt = ref(false);
const pgpPassphrase = ref("");
/** The sender email, from the chosen identity (drives the signing-key lookup). */
const fromEmail = computed(
  () =>
    identities.value.find((identity) => identity.id === selectedIdentityId.value)
      ?.email ?? "",
);
/** All recipient address strings, for the encryption-availability check. */
const pgpRecipients = computed(() =>
  [to.value, cc.value, bcc.value]
    .flatMap((raw) => raw.split(","))
    .map((part) => part.trim())
    .filter((part) => part.length > 0),
);
// Whether to request a read receipt (MDN) on send.
const requestReceipt = ref(false);

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
  } else if (draft === null && props.initial != null) {
    loadInitial(props.initial);
  }
  const loaded = await listIdentities();
  if (loaded !== null) {
    identities.value = loaded;
    preselectIdentity();
  }
  fields.value?.focus();
});

// Submission orchestration (build payload + send/schedule/save/autosave) lives
// in a composable so this shell stays under the size gate.
const { isComposerEmpty, onSend, onSchedule, onSaveDraft, saveAsTemplate, autosave } =
  useComposerSubmit({
    accountId,
    to,
    cc,
    bcc,
    subject,
    bodyText,
    bodyHtml,
    attachments,
    forwardedAttachments,
    inReplyTo,
    references,
    replySource,
    selectedIdentityId,
    draftId,
    requestReceipt,
    hasPrefill,
    currentSignatureText,
    sign,
    encrypt,
    pgpPassphrase,
    sending,
    savingDraft,
    finished,
    errorMessage,
    statusMessage,
    attachmentWarning,
    emitSent: (payload) => emit("sent", payload),
    emitSaved: (id) => emit("saved", id),
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

/** Fills a fresh composition from a template seed. Unlike a reply prefill the
 * body carries no placed signature, so signature handling stays enabled. */
function loadInitial(initial: ComposeInitial): void {
  to.value = (initial.to ?? []).join(", ");
  cc.value = (initial.cc ?? []).join(", ");
  bcc.value = (initial.bcc ?? []).join(", ");
  subject.value = initial.subject ?? "";
  bodyText.value = initial.body ?? "";
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
        <ComposePgpControls
          v-model:sign="sign"
          v-model:encrypt="encrypt"
          v-model:passphrase="pgpPassphrase"
          :from-email="fromEmail"
          :recipients="pgpRecipients"
        />
        <div class="options">
          <label class="receipt">
            <input
              v-model="requestReceipt"
              type="checkbox"
            >
            {{ $t("compose.requestReceipt") }}
          </label>
          <button
            type="button"
            class="template-button"
            @click="saveAsTemplate"
          >
            {{ $t("compose.saveAsTemplate") }}
          </button>
        </div>
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

.options {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.receipt {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.template-button {
  font-size: var(--text-sm);
  color: var(--color-accent);
  background: none;
  border: none;
  padding: var(--space-1) var(--space-2);
  cursor: pointer;
  border-radius: var(--radius-sm);
}

.template-button:hover {
  background: var(--color-bg-subtle);
}
</style>
