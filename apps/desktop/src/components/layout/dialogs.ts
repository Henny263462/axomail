import type { ComposeInitial, ComposerPrefillDto, DraftDto } from "../../types/mail";

/**
 * Payload for opening the composer: which account, an optional draft, an
 * optional reply/forward prefill, and an optional `initial` seed. A `prefill`
 * (from `prepare_reply`) seeds recipients, subject, body, threading headers, and
 * a forwarded attachment; `initial` seeds a fresh message from a template.
 */
export interface ComposerPayload {
  accountId: number;
  draft: DraftDto | null;
  prefill?: ComposerPrefillDto | null;
  initial?: ComposeInitial | null;
}

/** Payload for the move/copy folder picker: which messages, and which action. */
export interface MoveToFolderPayload {
  ids: number[];
  mode: "move" | "copy";
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
  addressBook: undefined;
  composer: ComposerPayload;
  moveToFolder: MoveToFolderPayload;
  /** The Ctrl/Cmd+K command palette. */
  commandPalette: undefined;
  /** The keyboard-shortcut help overlay (opened with `?`). */
  shortcutHelp: undefined;
  /** The message-template picker (New message from template). */
  templates: undefined;
};
