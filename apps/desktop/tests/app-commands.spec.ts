import { defineComponent, h } from "vue";
import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  buildAppCommands,
  buildContextCommands,
  chordBindingsFor,
  useAppCommands,
  type AppCommandDeps,
  type ContextCommandDeps,
} from "../src/composables/useAppCommands";
import { provideCommandRegistry } from "../src/composables/useCommandRegistry";

/** Dispatches a real keydown, defaulting the target to the window. */
function press(init: KeyboardEventInit, target: EventTarget = window): void {
  target.dispatchEvent(
    new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
  );
}

/** Wires useAppCommands under a real registry with spy callbacks. */
function setup(
  init: {
    openMessageId?: number | null;
    modalOpen?: boolean;
    accounts?: boolean;
  } = {},
) {
  const state = {
    // `?? 5` would turn an explicit `null` back into 5; test the key instead.
    openMessageId:
      init.openMessageId === undefined ? 5 : init.openMessageId,
    modalOpen: init.modalOpen ?? false,
    accounts: init.accounts ?? true,
  };
  const spies = {
    openCommandPalette: vi.fn(),
    openShortcutHelp: vi.fn(),
    newMessage: vi.fn(),
    reply: vi.fn(),
    replyAll: vi.fn(),
    forward: vi.fn(),
    archive: vi.fn(),
    deleteToTrash: vi.fn(),
    toggleStar: vi.fn(),
    toggleUnread: vi.fn(),
    focusSearch: vi.fn(),
    goInbox: vi.fn(),
    goFolders: vi.fn(),
  };
  const deps: AppCommandDeps = {
    isModalOpen: () => state.modalOpen,
    hasAccounts: () => state.accounts,
    getOpenMessageId: () => state.openMessageId,
    ...spies,
  };
  const Root = defineComponent({
    setup() {
      const registry = provideCommandRegistry();
      useAppCommands(registry, deps);
      return () => h("div");
    },
  });
  const wrapper = mount(Root, { attachTo: document.body });
  return { state, spies, wrapper };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("buildAppCommands catalogue", () => {
  const deps = Object.fromEntries(
    [
      "isModalOpen",
      "hasAccounts",
      "getOpenMessageId",
      "openCommandPalette",
      "openShortcutHelp",
      "newMessage",
      "reply",
      "replyAll",
      "forward",
      "archive",
      "deleteToTrash",
      "toggleStar",
      "toggleUnread",
      "focusSearch",
      "goInbox",
      "goFolders",
    ].map((k) => [k, () => undefined]),
  ) as unknown as AppCommandDeps;

  it("registers unique, namespaced ids with shortcuts", () => {
    const commands = buildAppCommands(deps);
    const ids = commands.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain("app.commandPalette");
    expect(ids).toContain("message.reply");
    expect(commands.every((c) => c.id.includes("."))).toBe(true);
  });

  it("exposes exactly the go-to sequences as chords", () => {
    const chords = chordBindingsFor(buildAppCommands(deps));
    expect(chords.map((c) => c.sequence)).toEqual([
      ["g", "i"],
      ["g", "f"],
    ]);
  });
});

describe("global shortcut dispatch", () => {
  it("triggers each single-key mail command", () => {
    const { spies, wrapper } = setup();
    press({ key: "n" });
    expect(spies.newMessage).toHaveBeenCalledOnce();
    press({ key: "r" });
    expect(spies.reply).toHaveBeenCalledWith(5);
    press({ key: "R", shiftKey: true });
    expect(spies.replyAll).toHaveBeenCalledWith(5);
    press({ key: "f" });
    expect(spies.forward).toHaveBeenCalledWith(5);
    press({ key: "e" });
    expect(spies.archive).toHaveBeenCalledWith(5);
    press({ key: "Delete" });
    expect(spies.deleteToTrash).toHaveBeenCalledWith(5);
    press({ key: "s" });
    expect(spies.toggleStar).toHaveBeenCalledWith(5);
    press({ key: "u" });
    expect(spies.toggleUnread).toHaveBeenCalledWith(5);
    press({ key: "/" });
    expect(spies.focusSearch).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("opens the palette (Mod+K) and help (?)", () => {
    const { spies, wrapper } = setup();
    press({ key: "k", ctrlKey: true });
    expect(spies.openCommandPalette).toHaveBeenCalledOnce();
    press({ key: "?", shiftKey: true });
    expect(spies.openShortcutHelp).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("does not trigger a bare mail key while typing in an input", () => {
    const { spies, wrapper } = setup();
    const input = document.createElement("input");
    document.body.appendChild(input);
    press({ key: "n" }, input);
    press({ key: "r" }, input);
    expect(spies.newMessage).not.toHaveBeenCalled();
    expect(spies.reply).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("dispatches the `g i` chord to go to the inbox", () => {
    const { spies, wrapper } = setup();
    press({ key: "g" });
    press({ key: "i" });
    expect(spies.goInbox).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("resolves `g f` to Folders, not the single-key Forward, with a message open", () => {
    // openMessageId defaults to 5, so the lone Forward binding is enabled and
    // would shadow the chord unless the dispatcher defers to the armed prefix.
    const { spies, wrapper } = setup();
    press({ key: "g" });
    press({ key: "f" });
    expect(spies.goFolders).toHaveBeenCalledOnce();
    expect(spies.forward).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("still forwards on a lone f when no chord prefix is armed", () => {
    const { spies, wrapper } = setup();
    press({ key: "f" });
    expect(spies.forward).toHaveBeenCalledWith(5);
    expect(spies.goFolders).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not fire a message command when no message is open", () => {
    const { spies, wrapper } = setup({ openMessageId: null });
    press({ key: "r" });
    press({ key: "e" });
    expect(spies.reply).not.toHaveBeenCalled();
    expect(spies.archive).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("suppresses every global shortcut while a modal is open", () => {
    const { spies, wrapper } = setup({ modalOpen: true });
    press({ key: "n" });
    press({ key: "r" });
    press({ key: "k", ctrlKey: true });
    press({ key: "g" });
    press({ key: "i" });
    expect(spies.newMessage).not.toHaveBeenCalled();
    expect(spies.reply).not.toHaveBeenCalled();
    expect(spies.openCommandPalette).not.toHaveBeenCalled();
    expect(spies.goInbox).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("does not start a new message with no account configured", () => {
    const { spies, wrapper } = setup({ accounts: false });
    press({ key: "n" });
    expect(spies.newMessage).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});

describe("buildContextCommands (address book / junk gating)", () => {
  function ctx(over: Partial<ContextCommandDeps> = {}): ContextCommandDeps {
    return {
      isModalOpen: () => false,
      getSelectedMessageId: () => 7,
      openAddressBook: vi.fn(),
      junk: vi.fn(),
      notJunk: vi.fn(),
      ...over,
    };
  }

  it("registers the address book, junk, and not-junk commands", () => {
    const ids = buildContextCommands(ctx()).map((c) => c.id);
    expect(ids).toEqual([
      "contacts.addressBook",
      "message.junk",
      "message.notJunk",
    ]);
  });

  it("gates every command on the modal state (none fire behind a dialog)", () => {
    for (const command of buildContextCommands(ctx())) {
      expect(command.isEnabled?.()).toBe(true);
    }
    const gated = buildContextCommands(ctx({ isModalOpen: () => true }));
    for (const command of gated) {
      expect(command.isEnabled?.()).toBe(false);
    }
  });

  it("dims junk / not-junk when nothing is selected", () => {
    const commands = buildContextCommands(ctx({ getSelectedMessageId: () => null }));
    const junk = commands.find((c) => c.id === "message.junk");
    const notJunk = commands.find((c) => c.id === "message.notJunk");
    expect(junk?.isEnabled?.()).toBe(false);
    expect(notJunk?.isEnabled?.()).toBe(false);
    // The address book has no selection requirement.
    expect(commands.find((c) => c.id === "contacts.addressBook")?.isEnabled?.()).toBe(
      true,
    );
  });

  it("runs junk / not-junk / address book against the current selection", () => {
    const junk = vi.fn();
    const notJunk = vi.fn();
    const openAddressBook = vi.fn();
    const commands = buildContextCommands(
      ctx({ junk, notJunk, openAddressBook, getSelectedMessageId: () => 42 }),
    );
    commands.find((c) => c.id === "message.junk")?.run();
    commands.find((c) => c.id === "message.notJunk")?.run();
    commands.find((c) => c.id === "contacts.addressBook")?.run();
    expect(junk).toHaveBeenCalledWith(42);
    expect(notJunk).toHaveBeenCalledWith(42);
    expect(openAddressBook).toHaveBeenCalledOnce();
  });
});
