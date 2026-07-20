/**
 * Registers axomail's global command set — the standard mail keyboard map and
 * the palette/help entry points — against the Stage-0 {@link useCommandRegistry}
 * and installs the go-to chord dispatcher. `App.vue` calls it once with plain
 * callbacks into the composables it already owns; this file re-binds no feature
 * logic, it only names the actions as commands so they gain a shortcut, a
 * palette entry, and a help listing.
 *
 * ## Two dispatch paths, one source of truth
 *
 * Single-chord commands (`n`, `r`, `Mod+K`, …) dispatch through the registry's
 * global keydown listener. Go-to chords (`g i`, `g f`) are expressed as
 * whitespace-separated shortcut specs; the registry's matcher is inert on those
 * (see {@link useKeyChords}), so they dispatch through {@link useKeyChords}
 * instead — while still appearing in the palette and help because they are
 * registered like any other command.
 *
 * ## Modal suppression
 *
 * Every command is gated on `!isModalOpen()` so the mail keys never fire behind
 * an open dialog. `isModalOpen()` deliberately excludes the palette itself: a
 * command run *from* the open palette must still be enabled, and the registry
 * already suppresses bare keys while the palette's search input holds focus.
 */
import {
  useKeyChords,
  isChordSequence,
  parseChordSequence,
  type ChordBinding,
} from "./useKeyChords";
import { i18n } from "../i18n";
import type { CommandRegistry } from "./useCommandRegistry";
import type { Command } from "../types/command";

/** The localized palette title for a command, from `app.command.<name>.title`. */
function title(name: string): string {
  return i18n.global.t(`app.command.${name}.title`);
}

/** The localized palette search terms for a command (comma-separated in the catalogue). */
function keywords(name: string): string[] {
  return i18n.global.t(`app.command.${name}.keywords`).split(",");
}

/** The callbacks and state gates the global commands drive. */
export interface AppCommandDeps {
  /** True while a blocking modal (not the palette) is open; gates every command. */
  isModalOpen: () => boolean;
  /** Whether at least one account exists (gates New message). */
  hasAccounts: () => boolean;
  /** The open message's id, or `null` when no message is displayed. */
  getOpenMessageId: () => number | null;
  /** Opens the command palette. */
  openCommandPalette: () => void;
  /** Opens the shortcut-help overlay. */
  openShortcutHelp: () => void;
  /** Starts a new message. */
  newMessage: () => void;
  /** Replies to the open message. */
  reply: (messageId: number) => void;
  /** Replies to all on the open message. */
  replyAll: (messageId: number) => void;
  /** Forwards the open message. */
  forward: (messageId: number) => void;
  /** Archives the open message. */
  archive: (messageId: number) => void;
  /** Moves the open message to Trash. */
  deleteToTrash: (messageId: number) => void;
  /** Toggles the open message's star. */
  toggleStar: (messageId: number) => void;
  /** Toggles the open message's read/unread state. */
  toggleUnread: (messageId: number) => void;
  /** Focuses the message search field. */
  focusSearch: () => void;
  /** Navigates to the unified inbox. */
  goInbox: () => void;
  /** Moves focus to the folder sidebar. */
  goFolders: () => void;
}

/**
 * Builds the command list from `deps`. Exported so unit tests can assert the
 * catalogue (ids, shortcuts, enabled gating, run wiring) without a component.
 */
export function buildAppCommands(deps: AppCommandDeps): Command[] {
  const notBusy = (): boolean => !deps.isModalOpen();
  const messageEnabled = (): boolean =>
    notBusy() && deps.getOpenMessageId() !== null;
  /** Wraps a message action so it runs against the currently open message. */
  const onMessage =
    (action: (messageId: number) => void): (() => void) =>
    () => {
      const id = deps.getOpenMessageId();
      if (id !== null) action(id);
    };

  return [
    {
      id: "app.commandPalette",
      title: title("commandPalette"),
      keywords: keywords("commandPalette"),
      shortcut: "Mod+K",
      run: deps.openCommandPalette,
      isEnabled: notBusy,
    },
    {
      id: "app.shortcutHelp",
      title: title("shortcutHelp"),
      keywords: keywords("shortcutHelp"),
      shortcut: "Shift+?",
      run: deps.openShortcutHelp,
      isEnabled: notBusy,
    },
    {
      id: "compose.new",
      title: title("newMessage"),
      keywords: keywords("newMessage"),
      shortcut: "n",
      run: deps.newMessage,
      isEnabled: () => notBusy() && deps.hasAccounts(),
    },
    {
      id: "message.reply",
      title: title("reply"),
      keywords: keywords("reply"),
      shortcut: "r",
      run: onMessage(deps.reply),
      isEnabled: messageEnabled,
    },
    {
      id: "message.replyAll",
      title: title("replyAll"),
      keywords: keywords("replyAll"),
      shortcut: "Shift+R",
      run: onMessage(deps.replyAll),
      isEnabled: messageEnabled,
    },
    {
      id: "message.forward",
      title: title("forward"),
      keywords: keywords("forward"),
      shortcut: "f",
      run: onMessage(deps.forward),
      isEnabled: messageEnabled,
    },
    {
      id: "message.archive",
      title: title("archive"),
      keywords: keywords("archive"),
      shortcut: "e",
      run: onMessage(deps.archive),
      isEnabled: messageEnabled,
    },
    {
      id: "message.delete",
      title: title("delete"),
      keywords: keywords("delete"),
      shortcut: "Delete",
      run: onMessage(deps.deleteToTrash),
      isEnabled: messageEnabled,
    },
    {
      id: "message.toggleStar",
      title: title("toggleStar"),
      keywords: keywords("toggleStar"),
      shortcut: "s",
      run: onMessage(deps.toggleStar),
      isEnabled: messageEnabled,
    },
    {
      id: "message.toggleUnread",
      title: title("toggleUnread"),
      keywords: keywords("toggleUnread"),
      shortcut: "u",
      run: onMessage(deps.toggleUnread),
      isEnabled: messageEnabled,
    },
    {
      id: "nav.focusSearch",
      title: title("search"),
      keywords: keywords("search"),
      shortcut: "/",
      run: deps.focusSearch,
      isEnabled: notBusy,
    },
    {
      id: "nav.goInbox",
      title: title("goInbox"),
      keywords: keywords("goInbox"),
      shortcut: "g i",
      run: deps.goInbox,
      isEnabled: notBusy,
    },
    {
      id: "nav.goFolders",
      title: title("goFolders"),
      keywords: keywords("goFolders"),
      shortcut: "g f",
      run: deps.goFolders,
      isEnabled: notBusy,
    },
  ];
}

/** Derives the chord bindings (go-to sequences) from the command catalogue. */
export function chordBindingsFor(commands: readonly Command[]): ChordBinding[] {
  return commands
    .filter(
      (command): command is Command & { shortcut: string } =>
        command.shortcut !== undefined && isChordSequence(command.shortcut),
    )
    .map((command) => ({
      sequence: parseChordSequence(command.shortcut),
      run: command.run,
      isEnabled: command.isEnabled,
    }));
}

/**
 * Registers the global commands against `registry` and wires the chord
 * dispatcher for the calling scope's lifetime. Call once, from the app root,
 * passing the {@link CommandRegistry} that `provideCommandRegistry()` returned
 * (a component cannot `inject` its own `provide`, so the registry is handed in
 * rather than re-injected here).
 */
export function useAppCommands(
  registry: CommandRegistry,
  deps: AppCommandDeps,
): void {
  const commands = buildAppCommands(deps);
  registry.registerAll(commands);
  const chords = useKeyChords(() => chordBindingsFor(commands));
  // Let the single-key dispatcher defer to an armed chord prefix so `g f`
  // resolves to the chord, never the lone Forward binding (finding: chord-vs-
  // single-key shadowing).
  registry.setChordPending(chords.isPending);
}

/**
 * Callbacks and gates for the *context* commands — the ones App.vue registers
 * outside the standard map because they close over dialogs / the list
 * selection: open address book, mark junk, mark not-junk. Kept here (beside
 * {@link buildAppCommands}) so their modal-gating is unit-testable rather than
 * inlined in `App.vue`.
 */
export interface ContextCommandDeps {
  /** True while a blocking modal is open; gates every context command. */
  isModalOpen: () => boolean;
  /** The list-selected message id, or `null` when nothing is selected. */
  getSelectedMessageId: () => number | null;
  /** Opens the address-book dialog. */
  openAddressBook: () => void;
  /** Files the selected message as junk. */
  junk: (messageId: number) => void;
  /** Restores the selected message from junk. */
  notJunk: (messageId: number) => void;
}

/**
 * Builds the context commands. Every one is gated on `!isModalOpen()` so a
 * junk/address-book shortcut never fires behind an open dialog, mirroring
 * {@link buildAppCommands}. Exported for unit tests.
 */
export function buildContextCommands(deps: ContextCommandDeps): Command[] {
  const notBusy = (): boolean => !deps.isModalOpen();
  const selectedEnabled = (): boolean =>
    notBusy() && deps.getSelectedMessageId() !== null;
  const onSelected =
    (action: (messageId: number) => void): (() => void) =>
    () => {
      const id = deps.getSelectedMessageId();
      if (id !== null) action(id);
    };

  return [
    {
      id: "contacts.addressBook",
      title: title("addressBook"),
      keywords: keywords("addressBook"),
      shortcut: "Mod+Shift+B",
      run: deps.openAddressBook,
      isEnabled: notBusy,
    },
    {
      id: "message.junk",
      title: title("markJunk"),
      keywords: keywords("markJunk"),
      shortcut: "j",
      run: onSelected(deps.junk),
      isEnabled: selectedEnabled,
    },
    {
      id: "message.notJunk",
      title: title("markNotJunk"),
      keywords: keywords("markNotJunk"),
      shortcut: "Shift+J",
      run: onSelected(deps.notJunk),
      isEnabled: selectedEnabled,
    },
  ];
}
