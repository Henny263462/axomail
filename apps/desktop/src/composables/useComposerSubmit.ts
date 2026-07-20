/**
 * Compose submission orchestration, split out of `ComposerDialog.vue` (size
 * gate) following the Wave-3 Stage-0 composable pattern. It owns building the
 * {@link ComposeInput} payload (merging the signature, wiring reply threading
 * headers and the two attachment transports) and the send / schedule / save /
 * autosave handlers. The dialog keeps the form state (refs) and passes them in.
 */
import type { ComputedRef, Ref } from "vue";

import { saveDraft, sendMessage } from "../api/compose";
import { markReplySource } from "../api/reply";
import { saveTemplate } from "../api/templates";
import { i18n } from "../i18n";
import { mentionsAttachmentKeyword, signatureHtml } from "../lib/htmlBody";
import { signatureBlock } from "../lib/signature";
import type {
  AttachmentCandidateDto,
  ComposeAttachmentDto,
  ComposeInput,
  PrefillSourceDto,
} from "../types/mail";

/** Reactive form state the submission logic reads and writes. */
export interface ComposerSubmitContext {
  accountId: Ref<number>;
  to: Ref<string>;
  cc: Ref<string>;
  bcc: Ref<string>;
  subject: Ref<string>;
  bodyText: Ref<string>;
  bodyHtml: Ref<string | null>;
  attachments: Ref<AttachmentCandidateDto[]>;
  forwardedAttachments: Ref<ComposeAttachmentDto[]>;
  inReplyTo: Ref<string | null>;
  references: Ref<string[]>;
  replySource: Ref<PrefillSourceDto | null>;
  selectedIdentityId: Ref<number | null>;
  draftId: Ref<number | null>;
  /** Whether to request a read receipt (MDN) on this send. */
  requestReceipt: Ref<boolean>;
  /** True for a reply/forward whose body already carries a placed signature. */
  hasPrefill: Ref<boolean>;
  currentSignatureText: ComputedRef<string>;
  /** OpenPGP: sign this message (Track 5.1). */
  sign: Ref<boolean>;
  /** OpenPGP: encrypt this message to its recipients. */
  encrypt: Ref<boolean>;
  /** Passphrase for a protected signing key; sent, never persisted. */
  pgpPassphrase: Ref<string>;
  sending: Ref<boolean>;
  savingDraft: Ref<boolean>;
  finished: Ref<boolean>;
  errorMessage: Ref<string>;
  statusMessage: Ref<string>;
  attachmentWarning: Ref<string>;
  emitSent: (payload: { draftId: number | null }) => void;
  emitSaved: (draftId: number) => void;
}

/** Splits a comma-separated recipient field into trimmed, non-empty parts. */
function splitAddresses(raw: string): string[] {
  return raw
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

export function useComposerSubmit(ctx: ComposerSubmitContext) {
  /** True when nothing worth keeping has been entered. */
  function isComposerEmpty(): boolean {
    return (
      [
        ctx.to.value,
        ctx.cc.value,
        ctx.bcc.value,
        ctx.subject.value,
        ctx.bodyText.value,
      ].every((value) => value.trim().length === 0) &&
      ctx.attachments.value.length === 0
    );
  }

  /**
   * Builds the compose payload, merging the identity signature into the body. A
   * reply/forward prefill already carries its signature, so it is not
   * re-appended (`hasPrefill`). Picked files ride `attachments` (paths); a
   * forwarded message rides `forwardedAttachments` (inline base64) — the backend
   * merges both.
   */
  function buildInput(scheduledAtUnix?: number): ComposeInput {
    const signature = ctx.hasPrefill.value ? "" : ctx.currentSignatureText.value;
    const html =
      ctx.bodyHtml.value === null
        ? null
        : ctx.bodyHtml.value + signatureHtml(signature);
    const input: ComposeInput = {
      accountId: ctx.accountId.value,
      to: splitAddresses(ctx.to.value),
      cc: splitAddresses(ctx.cc.value),
      bcc: splitAddresses(ctx.bcc.value),
      subject: ctx.subject.value.trim(),
      textBody: ctx.bodyText.value + signatureBlock(signature),
      htmlBody: html !== null && html.trim().length > 0 ? html : null,
      attachments: ctx.attachments.value.map((item) => item.path),
    };
    if (ctx.selectedIdentityId.value !== null) {
      input.identityId = ctx.selectedIdentityId.value;
    }
    if (scheduledAtUnix !== undefined) {
      input.scheduledAtUnix = scheduledAtUnix;
    }
    // Reply/forward context is applied only on the immediate-send path (the
    // backend does not persist it on the outbox row).
    if (ctx.inReplyTo.value !== null) {
      input.inReplyTo = ctx.inReplyTo.value;
    }
    if (ctx.references.value.length > 0) {
      input.references = ctx.references.value;
    }
    if (ctx.forwardedAttachments.value.length > 0) {
      input.forwardedAttachments = ctx.forwardedAttachments.value;
    }
    // OpenPGP protection (Track 5.1). Only carried when requested; the backend
    // applies it on the immediate-send path and rejects a deferred PGP send.
    if (ctx.sign.value) {
      input.sign = true;
      if (ctx.pgpPassphrase.value.length > 0) {
        input.pgpPassphrase = ctx.pgpPassphrase.value;
      }
    }
    if (ctx.encrypt.value) {
      input.encrypt = true;
    }
    if (ctx.requestReceipt.value) {
      input.requestReceipt = true;
    }
    return input;
  }

  /**
   * Saves the composer's current content as a reusable template, named after
   * the subject (falling back to a placeholder). Reports the outcome through the
   * shared status/error live regions.
   */
  async function saveAsTemplate(): Promise<void> {
    if (ctx.sending.value || ctx.savingDraft.value) return;
    ctx.errorMessage.value = "";
    const name =
      ctx.subject.value.trim() || i18n.global.t("app.composer.untitledTemplate");
    const template = await saveTemplate({
      name,
      subject: ctx.subject.value.trim(),
      to: splitAddresses(ctx.to.value),
      cc: splitAddresses(ctx.cc.value),
      bcc: splitAddresses(ctx.bcc.value),
      bodyText: ctx.bodyText.value,
    });
    if (template === null) {
      ctx.errorMessage.value = i18n.global.t("compose.templateUnavailable");
      return;
    }
    ctx.statusMessage.value = i18n.global.t("compose.templateSaved", {
      name: template.name,
    });
  }

  /** After a successful reply/forward send, marks the source message answered
   * ($Forwarded for a forward). A no-op for a plain new message or draft. */
  async function markSourceSent(): Promise<void> {
    const source = ctx.replySource.value;
    if (source === null) return;
    await markReplySource(source.messageId, source.mark);
  }

  /** True when the body hints at an attachment but none is attached. */
  function shouldRemindAttachment(): boolean {
    return (
      ctx.attachments.value.length === 0 &&
      (mentionsAttachmentKeyword(ctx.bodyText.value) ||
        mentionsAttachmentKeyword(ctx.subject.value))
    );
  }

  /** Submits a compose payload, immediately or scheduled. */
  async function submit(input: ComposeInput): Promise<void> {
    if (ctx.sending.value || ctx.savingDraft.value) return;
    ctx.statusMessage.value = "";
    if (input.to.length === 0) {
      ctx.errorMessage.value = i18n.global.t("app.composer.noRecipients");
      return;
    }
    // Attachment-keyword reminder: warn once, then honour a repeated send.
    if (ctx.attachmentWarning.value === "" && shouldRemindAttachment()) {
      ctx.attachmentWarning.value = i18n.global.t("app.composer.attachmentReminder");
      return;
    }
    ctx.attachmentWarning.value = "";
    ctx.errorMessage.value = "";
    ctx.sending.value = true;
    try {
      const outcome = await sendMessage(input);
      if (outcome === null) {
        ctx.errorMessage.value =
          "Sending needs the desktop app; it is unavailable in the browser preview.";
        return;
      }
      // Only a message that actually left the machine marks its source answered
      // or forwarded (the flag reflects a delivered reply, not a queued one).
      if (outcome.sent) {
        await markSourceSent();
        ctx.finished.value = true;
        ctx.emitSent({ draftId: ctx.draftId.value });
        return;
      }
      // A scheduled or offline-queued message never leaves now, but it did leave
      // the composer: it waits in the outbox (offline sends flush on reconnect).
      if (
        (outcome.scheduledAtUnix ?? null) !== null ||
        outcome.queuedOffline === true
      ) {
        ctx.finished.value = true;
        ctx.emitSent({ draftId: ctx.draftId.value });
        return;
      }
      ctx.errorMessage.value = outcome.error ?? "The message could not be sent.";
    } finally {
      ctx.sending.value = false;
    }
  }

  function onSend(): Promise<void> {
    return submit(buildInput());
  }

  function onSchedule(unixSeconds: number): Promise<void> {
    return submit(buildInput(unixSeconds));
  }

  async function onSaveDraft(): Promise<void> {
    if (ctx.savingDraft.value || ctx.sending.value) return;
    ctx.errorMessage.value = "";
    ctx.savingDraft.value = true;
    try {
      const id = await saveDraft(buildInput(), ctx.draftId.value);
      if (id === null) {
        ctx.errorMessage.value =
          "Saving needs the desktop app; it is unavailable in the browser preview.";
        return;
      }
      ctx.draftId.value = id;
      ctx.statusMessage.value = "Draft saved.";
      ctx.emitSaved(id);
    } finally {
      ctx.savingDraft.value = false;
    }
  }

  /** Timer-driven save used by `useDraftAutosave`; quiet on failure. */
  async function autosave(): Promise<void> {
    const id = await saveDraft(buildInput(), ctx.draftId.value);
    if (id !== null) {
      ctx.draftId.value = id;
      ctx.statusMessage.value = "Draft autosaved.";
    }
  }

  return {
    buildInput,
    isComposerEmpty,
    onSend,
    onSchedule,
    onSaveDraft,
    saveAsTemplate,
    autosave,
  };
}
