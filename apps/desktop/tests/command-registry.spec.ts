/* eslint-disable vue/one-component-per-file --
   Several tiny throwaway host/child components exercise the provide/inject and
   auto-dispose behaviour; the one-component-per-file hygiene rule is for SFCs. */
import { defineComponent, h, nextTick } from "vue";
import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Command } from "../src/types/command";
import {
  createCommandRegistry,
  formatShortcut,
  parseShortcut,
  provideCommandRegistry,
  useCommandRegistry,
  type CommandRegistry,
} from "../src/composables/useCommandRegistry";

/** A no-op command with sensible defaults, overridable per test. */
function cmd(overrides: Partial<Command> & { id: string }): Command {
  return {
    title: overrides.id,
    keywords: [],
    run: () => {},
    ...overrides,
  };
}

/** Builds a keydown event; `target` defaults to the document body (non-editable). */
function keydown(
  init: KeyboardEventInit,
  target: EventTarget = document.body,
): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { cancelable: true, ...init });
  Object.defineProperty(event, "target", { value: target, configurable: true });
  return event;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("createCommandRegistry — registration", () => {
  it("lists registered commands in registration order", () => {
    const registry = createCommandRegistry();
    registry.register(cmd({ id: "b" }));
    registry.register(cmd({ id: "a" }));
    expect(registry.commands.value.map((c) => c.id)).toEqual(["b", "a"]);
  });

  it("re-registering an id replaces the command, keeping its slot", () => {
    const registry = createCommandRegistry();
    registry.register(cmd({ id: "x", title: "first" }));
    registry.register(cmd({ id: "x", title: "second" }));
    expect(registry.commands.value).toHaveLength(1);
    expect(registry.get("x")?.title).toBe("second");
  });

  it("the unregister fn removes only its own registration", () => {
    const registry = createCommandRegistry();
    const off = registry.register(cmd({ id: "x", title: "first" }));
    // A newer registration of the same id supersedes the first.
    registry.register(cmd({ id: "x", title: "second" }));
    off(); // stale disposer must not drop the current command
    expect(registry.get("x")?.title).toBe("second");
  });

  it("registerAll returns one disposer that removes every command", () => {
    const registry = createCommandRegistry();
    const off = registry.registerAll([cmd({ id: "a" }), cmd({ id: "b" })]);
    expect(registry.commands.value).toHaveLength(2);
    off();
    expect(registry.commands.value).toHaveLength(0);
  });
});

describe("createCommandRegistry — keydown dispatch", () => {
  it("runs a bare single-key command and prevents the default", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "new", shortcut: "n", run }));

    const event = keydown({ key: "n" });
    expect(registry.handleKeydown(event)).toBe(true);
    expect(run).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it("does not fire a bare key when a modifier is held", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "new", shortcut: "n", run }));
    expect(registry.handleKeydown(keydown({ key: "n", ctrlKey: true }))).toBe(
      false,
    );
    expect(run).not.toHaveBeenCalled();
  });

  it("matches Mod+K against Ctrl on a non-mac platform", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "palette", shortcut: "Mod+K", run }));
    // happy-dom reports a non-mac platform.
    expect(registry.handleKeydown(keydown({ key: "k", ctrlKey: true }))).toBe(
      true,
    );
    expect(run).toHaveBeenCalledOnce();
  });

  it("ignores an event another handler already claimed", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "new", shortcut: "n", run }));
    const event = keydown({ key: "n" });
    event.preventDefault(); // a closer widget claimed it first
    expect(registry.handleKeydown(event)).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });

  it("returns false when no shortcut matches", () => {
    const registry = createCommandRegistry();
    registry.register(cmd({ id: "new", shortcut: "n", run: vi.fn() }));
    expect(registry.handleKeydown(keydown({ key: "z" }))).toBe(false);
  });

  it("does not fire a disabled command; the keystroke falls through", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(
      cmd({ id: "reply", shortcut: "r", run, isEnabled: () => false }),
    );
    const event = keydown({ key: "r" });
    expect(registry.handleKeydown(event)).toBe(false);
    expect(run).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});

describe("createCommandRegistry — text-input focus", () => {
  it("never hijacks a bare key while typing in an input", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "new", shortcut: "n", run }));

    const input = document.createElement("input");
    document.body.appendChild(input);
    const event = keydown({ key: "n" }, input);
    expect(registry.handleKeydown(event)).toBe(false);
    expect(run).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("still dispatches a modifier chord from within an input", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "palette", shortcut: "Mod+K", run }));

    const input = document.createElement("input");
    document.body.appendChild(input);
    expect(
      registry.handleKeydown(keydown({ key: "k", ctrlKey: true }, input)),
    ).toBe(true);
    expect(run).toHaveBeenCalledOnce();
  });

  it("suppresses a bare key inside a contenteditable region", () => {
    const registry = createCommandRegistry();
    const run = vi.fn();
    registry.register(cmd({ id: "archive", shortcut: "a", run }));

    const editor = document.createElement("div");
    editor.contentEditable = "true";
    document.body.appendChild(editor);
    // Guard against a happy-dom build without isContentEditable support.
    if (!editor.isContentEditable) editor.setAttribute("contenteditable", "true");
    expect(registry.handleKeydown(keydown({ key: "a" }, editor))).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });
});

describe("parseShortcut / formatShortcut", () => {
  it("parses modifiers and the key, case-insensitively", () => {
    expect(parseShortcut("Ctrl+Shift+A")).toMatchObject({
      key: "a",
      ctrl: true,
      shift: true,
      alt: false,
      meta: false,
      mod: false,
    });
  });

  it("maps Cmd/Command/Super/Win onto the meta flag", () => {
    for (const spec of ["Cmd+K", "Command+K", "Super+K", "Win+K"]) {
      expect(parseShortcut(spec)?.meta).toBe(true);
    }
  });

  it("normalizes the space key both ways", () => {
    expect(parseShortcut("Space")?.key).toBe("space");
  });

  it("returns null for a spec that names no key", () => {
    expect(parseShortcut("Ctrl+")).toBeNull();
  });

  it("formats a chord for display on a non-mac platform", () => {
    expect(formatShortcut("Mod+K")).toBe("Ctrl+K");
    expect(formatShortcut("Ctrl+Shift+A")).toBe("Ctrl+Shift+A");
    expect(formatShortcut("Delete")).toBe("Delete");
  });
});

describe("provide/useCommandRegistry integration", () => {
  it("shares one registry and dispatches real window keydowns", async () => {
    const run = vi.fn();
    const Child = defineComponent({
      setup() {
        const registry = useCommandRegistry();
        registry.register(cmd({ id: "new", shortcut: "n", run }));
        return () => h("div", "child");
      },
    });
    const Root = defineComponent({
      setup() {
        provideCommandRegistry();
        return () => h(Child);
      },
    });

    const wrapper = mount(Root, { attachTo: document.body });
    await nextTick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "n" }));
    expect(run).toHaveBeenCalledOnce();

    // Unmount removes both the registration and the global listener.
    wrapper.unmount();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "n" }));
    expect(run).toHaveBeenCalledOnce();
  });

  it("unregisters a component's commands when it unmounts", async () => {
    let registry: CommandRegistry | null = null;
    const Feature = defineComponent({
      setup() {
        useCommandRegistry().register(cmd({ id: "feature.action" }));
        return () => h("div");
      },
    });
    const Root = defineComponent({
      props: { showFeature: { type: Boolean, default: true } },
      setup(props) {
        registry = provideCommandRegistry();
        return () => (props.showFeature ? h(Feature) : h("div", "gone"));
      },
    });

    const wrapper = mount(Root);
    await nextTick();
    expect(registry!.commands.value).toHaveLength(1);

    await wrapper.setProps({ showFeature: false });
    await nextTick();
    expect(registry!.commands.value).toHaveLength(0);
  });

  it("throws when used without a provider ancestor", () => {
    const Lonely = defineComponent({
      setup() {
        useCommandRegistry();
        return () => h("div");
      },
    });
    expect(() => mount(Lonely)).toThrow(/provideCommandRegistry/);
  });
});
