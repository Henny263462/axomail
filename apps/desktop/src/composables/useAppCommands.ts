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
import type { CommandRegistry } from "./useCommandRegistry";
import type { Command } from "../types/command";

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
      title: "Command palette",
      keywords: ["commands", "run", "actions"],
      shortcut: "Mod+K",
      run: deps.openCommandPalette,
      isEnabled: notBusy,
    },
    {
      id: "app.shortcutHelp",
      title: "Keyboard shortcuts",
      keywords: ["help", "keys", "cheat sheet"],
      shortcut: "Shift+?",
      run: deps.openShortcutHelp,
      isEnabled: notBusy,
    },
    {
      id: "compose.new",
      title: "New message",
      keywords: ["compose", "write", "create"],
      shortcut: "n",
      run: deps.newMessage,
      isEnabled: () => notBusy() && deps.hasAccounts(),
    },
    {
      id: "message.reply",
      title: "Reply",
      keywords: ["respond"],
      shortcut: "r",
      run: onMessage(deps.reply),
      isEnabled: messageEnabled,
    },
    {
      id: "message.replyAll",
      title: "Reply all",
      keywords: ["respond", "everyone"],
      shortcut: "Shift+R",
      run: onMessage(deps.replyAll),
      isEnabled: messageEnabled,
    },
    {
      id: "message.forward",
      title: "Forward",
      keywords: ["fwd"],
      shortcut: "f",
      run: onMessage(deps.forward),
      isEnabled: messageEnabled,
    },
    {
      id: "message.archive",
      title: "Archive",
      keywords: ["file"],
      shortcut: "e",
      run: onMessage(deps.archive),
      isEnabled: messageEnabled,
    },
    {
      id: "message.delete",
      title: "Delete to Trash",
      keywords: ["trash", "remove"],
      shortcut: "Delete",
      run: onMessage(deps.deleteToTrash),
      isEnabled: messageEnabled,
    },
    {
      id: "message.toggleStar",
      title: "Toggle star",
      keywords: ["flag", "favourite", "favorite"],
      shortcut: "s",
      run: onMessage(deps.toggleStar),
      isEnabled: messageEnabled,
    },
    {
      id: "message.toggleUnread",
      title: "Toggle unread",
      keywords: ["read", "seen", "mark"],
      shortcut: "u",
      run: onMessage(deps.toggleUnread),
      isEnabled: messageEnabled,
    },
    {
      id: "nav.focusSearch",
      title: "Search messages",
      keywords: ["find", "filter"],
      shortcut: "/",
      run: deps.focusSearch,
      isEnabled: notBusy,
    },
    {
      id: "nav.goInbox",
      title: "Go to Inbox",
      keywords: ["unified", "inbox", "home"],
      shortcut: "g i",
      run: deps.goInbox,
      isEnabled: notBusy,
    },
    {
      id: "nav.goFolders",
      title: "Go to Folders",
      keywords: ["sidebar", "mailboxes", "accounts"],
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
  useKeyChords(() => chordBindingsFor(commands));
}
