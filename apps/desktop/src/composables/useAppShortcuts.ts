/**
 * App-level command & keyboard-shortcut wiring, extracted from `App.vue` so the
 * shell stays under the file-size gate (Wave 5 Stage 0 headroom). It owns the
 * command registry lifecycle, the modal-state predicate, the two focus helpers,
 * and the registration of both the standard mail command map and the context
 * commands (address book / junk).
 *
 * `App.vue` calls it once with the sub-composables it already owns and uses the
 * returned {@link isModalOpen} both to gate shortcuts and to tell the message
 * pane whether a modal is up. No behaviour changes: this only relocates the
 * wiring that previously lived inline in the shell.
 */
import { nextTick, type ComputedRef, type Ref } from "vue";

import { buildContextCommands, useAppCommands } from "./useAppCommands";
import { provideCommandRegistry } from "./useCommandRegistry";
import { i18n } from "../i18n";
import type { Pane } from "./usePaneNavigation";
import { useToolbarCommands } from "./useToolbarCommands";
import type { ForwardStyle } from "../types/compose";
import type { DialogController } from "./useDialogs";
import type { AppDialogMap } from "../components/layout/dialogs";
import type { AccountSummary } from "../types/mail";

/** Compose-entry actions used by the mail command map. */
interface ComposeEntry {
  openCompose: () => void;
  openReply: (id: number) => void | Promise<void>;
  openReplyAll: (id: number) => void | Promise<void>;
  openForward: (id: number, style?: ForwardStyle) => void | Promise<void>;
}

/** Bulk message actions (id lists), used single-target here. */
interface MailboxActionSet {
  archive: (ids: number[]) => void | Promise<void>;
  remove: (ids: number[]) => void | Promise<void>;
  junk: (ids: number[]) => void | Promise<void>;
  notJunk: (ids: number[]) => void | Promise<void>;
  requestMove: (ids: number[]) => void;
  requestCopy: (ids: number[]) => void;
}

/** Optimistic per-message toggles. */
interface MessageToggles {
  toggleStar: (id: number) => void | Promise<void>;
  toggleSeen: (id: number) => void | Promise<void>;
}

/** The message-backed view routing the shortcuts read/drive. */
interface MailboxViewNav {
  viewedMessageId: ComputedRef<number | null>;
  selectUnified: () => void | Promise<void>;
}

/** Everything the shortcut wiring needs from the shell. */
export interface AppShortcutsDeps {
  dialogs: DialogController<AppDialogMap>;
  accounts: Ref<AccountSummary[]>;
  selectedMessageId: Ref<number | null>;
  /** Reveals a pane on the narrow, stacked layout (`shell.showPane`). */
  showPane: (pane: Pane) => void;
  view: MailboxViewNav;
  compose: ComposeEntry;
  mailboxActions: MailboxActionSet;
  messageActions: MessageToggles;
}

/** What the shell consumes back: the modal predicate for the message pane. */
export interface AppShortcuts {
  /** True while a blocking modal (not the palette) is open. */
  isModalOpen: () => boolean;
}

/**
 * Installs the command registry, registers the global + context commands, and
 * returns the modal predicate. Call once from `App.vue`'s setup so the
 * registry `provide()` and its window keydown dispatcher bind to the shell.
 */
export function useAppShortcuts(deps: AppShortcutsDeps): AppShortcuts {
  const {
    dialogs,
    accounts,
    selectedMessageId,
    showPane,
    view,
    compose,
    mailboxActions,
    messageActions,
  } = deps;

  // App-wide command registry + global keyboard dispatcher, provided here so
  // descendants (palette, help overlay) read it via useCommandRegistry().
  const registry = provideCommandRegistry();

  /**
   * True while a blocking modal is open — the palette is excluded so a command
   * run from it stays enabled (bare keys are already suppressed by the
   * palette's focused search input). Gates every global shortcut. Any app-level
   * dialog that should suppress background shortcuts belongs here.
   */
  function isModalOpen(): boolean {
    return (
      dialogs.isOpen("composer") ||
      dialogs.isOpen("settings") ||
      dialogs.isOpen("setup") ||
      dialogs.isOpen("activity") ||
      dialogs.isOpen("addressBook") ||
      dialogs.isOpen("moveToFolder") ||
      dialogs.isOpen("shortcutHelp") ||
      dialogs.isOpen("advancedSearch") ||
      dialogs.isOpen("templates")
    );
  }

  /** Reveals the list pane and focuses its search field (the `/` shortcut). */
  function focusSearch(): void {
    showPane("list");
    void nextTick(() => {
      document
        .querySelector<HTMLInputElement>('input[role="searchbox"]')
        ?.focus();
    });
  }

  /**
   * Reveals and focuses the folder sidebar (the `g f` chord). Selects the nav
   * by its stable `.sidebar` class, not its (now localized) aria-label, so the
   * shortcut works in every language.
   */
  function focusSidebar(): void {
    showPane("sidebar");
    void nextTick(() => {
      document.querySelector<HTMLElement>("nav.sidebar button")?.focus();
    });
  }

  // Context commands: address book (Mod+Shift+B), and junk "j" / not-junk
  // "Shift+J" acting on the list selection. All modal-gated so none fires behind
  // an open dialog; registration auto-disposes with the calling component.
  registry.registerAll(
    buildContextCommands({
      isModalOpen,
      getSelectedMessageId: () => selectedMessageId.value,
      openAddressBook: () => dialogs.open("addressBook"),
      junk: (id) => void mailboxActions.junk([id]),
      notJunk: (id) => void mailboxActions.notJunk([id]),
    }),
  );

  // "New message from template" — palette-only (no default shortcut); opens the
  // template picker, which then opens the composer pre-filled. Modal-gated and
  // auto-disposed with the calling component like the other registrations.
  registry.register({
    id: "compose.newFromTemplate",
    title: i18n.global.t("templates.command"),
    keywords: ["template", "templates", "boilerplate", "canned"],
    run: () => dialogs.open("templates"),
    isEnabled: () => !isModalOpen(),
  });

  useToolbarCommands(registry, {
    isModalOpen,
    accounts,
    openMessageId: view.viewedMessageId,
    openSettings: () => dialogs.open("settings"),
    reply: (id) => void compose.openReply(id),
    replyAll: (id) => void compose.openReplyAll(id),
    forward: (id, style) => void compose.openForward(id, style),
    archive: (id) => void mailboxActions.archive([id]),
    move: (id) => mailboxActions.requestMove([id]),
    copy: (id) => mailboxActions.requestCopy([id]),
    junk: (id) => void mailboxActions.junk([id]),
    remove: (id) => void mailboxActions.remove([id]),
  });

  // The global mail shortcut map + go-to chords.
  useAppCommands(registry, {
    isModalOpen,
    hasAccounts: () => accounts.value.length > 0,
    getOpenMessageId: () => view.viewedMessageId.value,
    openCommandPalette: () => dialogs.open("commandPalette"),
    openShortcutHelp: () => dialogs.open("shortcutHelp"),
    newMessage: compose.openCompose,
    reply: (id) => void compose.openReply(id),
    replyAll: (id) => void compose.openReplyAll(id),
    forward: (id) => void compose.openForward(id),
    archive: (id) => void mailboxActions.archive([id]),
    deleteToTrash: (id) => void mailboxActions.remove([id]),
    toggleStar: (id) => void messageActions.toggleStar(id),
    toggleUnread: (id) => void messageActions.toggleSeen(id),
    focusSearch,
    goInbox: () => void view.selectUnified(),
    goFolders: focusSidebar,
  });

  return { isModalOpen };
}
