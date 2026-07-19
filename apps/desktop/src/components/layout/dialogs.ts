import type { ComposerPrefillDto, DraftDto } from "../../types/mail";

/**
 * Payload for opening the composer: which account, an optional draft, and an
 * optional reply/forward prefill. A `prefill` (from `prepare_reply`) seeds the
 * recipients, subject, body, threading headers, and a forwarded attachment.
 */
export interface ComposerPayload {
  accountId: number;
  draft: DraftDto | null;
  prefill?: ComposerPrefillDto | null;
}

/**
 * The application's app-level dialogs and their open payloads (`undefined`
 * for dialogs that need none). Adding a dialog is: a new SFC, one branch in
 * {@link DialogHost}, and one entry here — `App.vue` stays untouched beyond
 * the single call that opens it.
 */
export type AppDialogMap = {
  setup: undefined;
  settings: undefined;
  activity: undefined;
  composer: ComposerPayload;
};
