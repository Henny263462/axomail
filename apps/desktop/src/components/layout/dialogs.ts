import type { DraftDto } from "../../types/mail";

/** Payload for opening the composer: which account, and an optional draft. */
export interface ComposerPayload {
  accountId: number;
  draft: DraftDto | null;
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
  composer: ComposerPayload;
};
