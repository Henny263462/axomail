import { defineComponent, h, nextTick } from "vue";
import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import CommandPalette from "../src/components/command/CommandPalette.vue";
import ShortcutHelp from "../src/components/command/ShortcutHelp.vue";
import ShortcutsSection from "../src/components/settings/ShortcutsSection.vue";
import { provideCommandRegistry } from "../src/composables/useCommandRegistry";
import type { Command } from "../src/types/command";

/** A command with sensible defaults, overridable per test. */
function cmd(over: Partial<Command> & { id: string }): Command {
  return { title: over.id, keywords: [], run: () => {}, ...over };
}

/** Mounts `child` under a host that provides a registry seeded with `commands`. */
function hostFor(
  child: unknown,
  commands: Command[],
  props: Record<string, unknown> = {},
) {
  const Root = defineComponent({
    setup() {
      const registry = provideCommandRegistry();
      registry.registerAll(commands);
      return () => h(child as never, props);
    },
  });
  return mount(Root, { attachTo: document.body });
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("CommandPalette", () => {
  function palette(commands: Command[]) {
    const onClose = vi.fn();
    const wrapper = hostFor(CommandPalette, commands, { onClose });
    return { wrapper, onClose };
  }

  const sample = (): Command[] => [
    cmd({ id: "compose.new", title: "New message", keywords: ["write"], shortcut: "n" }),
    cmd({ id: "message.reply", title: "Reply", shortcut: "r" }),
    cmd({ id: "nav.goInbox", title: "Go to Inbox", shortcut: "g i" }),
  ];

  it("renders the combobox/listbox structure with every command", () => {
    const { wrapper } = palette(sample());
    expect(wrapper.find('input[role="combobox"]').exists()).toBe(true);
    expect(wrapper.find('[role="listbox"]').exists()).toBe(true);
    expect(wrapper.findAll('[role="option"]')).toHaveLength(3);
    // The go-to chord renders as a "then" phrase.
    expect(wrapper.text()).toContain("G then I");
  });

  it("filters as the query changes", async () => {
    const { wrapper } = palette(sample());
    await wrapper.find("input").setValue("reply");
    const options = wrapper.findAll('[role="option"]');
    expect(options).toHaveLength(1);
    expect(options[0]!.text()).toContain("Reply");
  });

  it("shows an honest empty state when nothing matches", async () => {
    const { wrapper } = palette(sample());
    await wrapper.find("input").setValue("zzzzz");
    expect(wrapper.findAll('[role="option"]')).toHaveLength(0);
    expect(wrapper.get('[role="status"]').text()).toContain(
      "No matching commands",
    );
  });

  it("moves the active option with the arrow keys", async () => {
    const { wrapper } = palette(sample());
    const input = wrapper.find("input");
    // First option is active initially.
    expect(wrapper.findAll('[role="option"]')[0]!.attributes("aria-selected")).toBe(
      "true",
    );
    await input.trigger("keydown", { key: "ArrowDown" });
    expect(wrapper.findAll('[role="option"]')[1]!.attributes("aria-selected")).toBe(
      "true",
    );
    // Wraps from the top back to the bottom.
    await input.trigger("keydown", { key: "ArrowUp" });
    await input.trigger("keydown", { key: "ArrowUp" });
    expect(wrapper.findAll('[role="option"]')[2]!.attributes("aria-selected")).toBe(
      "true",
    );
  });

  it("runs the active command and closes on Enter", async () => {
    const run = vi.fn();
    const { wrapper, onClose } = palette([
      cmd({ id: "message.reply", title: "Reply", shortcut: "r", run }),
      cmd({ id: "compose.new", title: "New message", shortcut: "n" }),
    ]);
    await wrapper.find("input").setValue("reply");
    await wrapper.find("input").trigger("keydown", { key: "Enter" });
    expect(run).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("runs a command on click", async () => {
    const run = vi.fn();
    const { wrapper, onClose } = palette([
      cmd({ id: "message.reply", title: "Reply", run }),
    ]);
    await wrapper.find('[role="option"]').trigger("click");
    expect(run).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("does not run a disabled command", async () => {
    const run = vi.fn();
    const { wrapper, onClose } = palette([
      cmd({
        id: "message.reply",
        title: "Reply",
        run,
        isEnabled: () => false,
      }),
    ]);
    await wrapper.find('[role="option"]').trigger("click");
    expect(run).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(wrapper.find('[role="option"]').attributes("aria-disabled")).toBe(
      "true",
    );
  });

  it("closes on Escape", async () => {
    const { wrapper, onClose } = palette(sample());
    await wrapper.find("input").trigger("keydown", { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("keeps aria-expanded true even when nothing matches (popup still shown)", async () => {
    const { wrapper } = palette(sample());
    const input = wrapper.get('input[role="combobox"]');
    expect(input.attributes("aria-expanded")).toBe("true");
    await input.setValue("zzzzz");
    expect(wrapper.findAll('[role="option"]')).toHaveLength(0);
    // The no-results popup is still visible, so it must not report collapsed.
    expect(input.attributes("aria-expanded")).toBe("true");
  });

  it("traps Tab so focus cannot escape to the background", () => {
    const { wrapper } = palette(sample());
    const input = wrapper.get('input[role="combobox"]').element as HTMLInputElement;
    input.focus();
    const event = new KeyboardEvent("keydown", {
      key: "Tab",
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(input);
  });
});

describe("ShortcutHelp", () => {
  const commands = (): Command[] => [
    cmd({ id: "compose.new", title: "New message", shortcut: "n" }),
    cmd({ id: "message.reply", title: "Reply", shortcut: "r" }),
    cmd({ id: "nav.goInbox", title: "Go to Inbox", shortcut: "g i" }),
    // A command without a shortcut is omitted from the help listing.
    cmd({ id: "message.noKey", title: "No shortcut" }),
  ];

  it("lists bound shortcuts grouped by area", () => {
    const wrapper = hostFor(ShortcutHelp, commands(), { onClose: vi.fn() });
    const text = wrapper.text();
    expect(text).toContain("Compose");
    expect(text).toContain("Message");
    expect(text).toContain("Navigation");
    expect(text).toContain("New message");
    expect(text).toContain("G then I");
    expect(text).not.toContain("No shortcut");
  });

  it("emits close from the Close button and Escape", async () => {
    const onClose = vi.fn();
    const wrapper = hostFor(ShortcutHelp, commands(), { onClose });
    await wrapper.find('button[aria-label="Close"]').trigger("click");
    await wrapper.find('[role="dialog"]').trigger("keydown", { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("restores focus to the invoker when it closes", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    const wrapper = hostFor(ShortcutHelp, commands(), { onClose: vi.fn() });
    await nextTick();
    // Focus moved into the overlay on open.
    expect(document.activeElement).not.toBe(trigger);

    wrapper.unmount();
    // …and returns to the invoker on close.
    expect(document.activeElement).toBe(trigger);
  });
});

describe("ShortcutsSection (settings)", () => {
  it("lists the bound shortcuts read-only", async () => {
    const context = {} as never;
    const wrapper = hostFor(
      ShortcutsSection,
      [
        cmd({ id: "compose.new", title: "New message", shortcut: "n" }),
        cmd({ id: "message.reply", title: "Reply", shortcut: "r" }),
      ],
      { context },
    );
    await nextTick();
    const text = wrapper.text();
    expect(text).toContain("Keyboard shortcuts");
    expect(text).toContain("New message");
    expect(text).toContain("Reply");
  });
});
