import { defineComponent, h, nextTick, ref } from "vue";
import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import DropdownMenu from "../src/components/common/DropdownMenu.vue";
import MainToolbar from "../src/components/layout/MainToolbar.vue";
import { framedHtml } from "../src/components/message/htmlFrame";
import MessageBody from "../src/components/message/MessageBody.vue";
import { provideCommandRegistry } from "../src/composables/useCommandRegistry";
import { syncAllAccounts, useToolbarCommands } from "../src/composables/useToolbarCommands";
import { messageWindowId } from "../src/lib/messageWindow";
import type { Command } from "../src/types/command";

vi.mock("../src/api/sync", () => ({
  syncAccount: vi.fn(() => Promise.resolve([])),
}));

import { syncAccount } from "../src/api/sync";

function cmd(over: Partial<Command> & { id: string }): Command {
  return { title: over.id, keywords: [], run: () => {}, ...over };
}

function host(setup: (registry: ReturnType<typeof provideCommandRegistry>) => void, child: unknown = "div") {
  const Root = defineComponent({
    setup() {
      setup(provideCommandRegistry());
      return () => h(child as never);
    },
  });
  return mount(Root, { attachTo: document.body });
}

function toolbarWith(commands: Command[]) {
  return host((registry) => registry.registerAll(commands), MainToolbar);
}

afterEach(() => {
  document.body.innerHTML = "";
  localStorage.clear();
});

describe("MainToolbar", () => {
  it("runs the registered command for a button", async () => {
    const reply = vi.fn();
    const wrapper = toolbarWith([cmd({ id: "message.reply", run: reply })]);
    const button = wrapper.findAll("button.tool").find((b) => b.text() === "Reply");
    if (button === undefined) throw new Error("reply button missing");
    await button.trigger("click");
    expect(reply).toHaveBeenCalledOnce();
  });

  it("disables buttons whose command is missing or disabled", () => {
    const wrapper = toolbarWith([cmd({ id: "message.delete", isEnabled: () => false })]);
    const remove = wrapper.findAll("button.tool").find((b) => b.text() === "Delete");
    expect(remove?.attributes("disabled")).toBeDefined();
    const settings = wrapper.findAll("button.tool").find((b) => b.text() === "Settings");
    expect(settings?.attributes("disabled")).toBeDefined();
  });

  it("offers the forward variants in a menu", async () => {
    const inline = vi.fn();
    const wrapper = toolbarWith([
      cmd({ id: "message.forward" }),
      cmd({ id: "message.forwardInline", run: inline }),
      cmd({ id: "message.forwardAttachment" }),
    ]);
    const trigger = wrapper.findAll(".split .trigger")[1];
    await trigger?.trigger("click");
    await nextTick();
    const item = wrapper.findAll('[role="menuitem"]').find((b) => b.text() === "Forward inline");
    await item?.trigger("click");
    expect(inline).toHaveBeenCalledOnce();
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
  });
});

describe("DropdownMenu", () => {
  it("opens, closes on Escape and skips disabled items", async () => {
    const wrapper = mount(DropdownMenu, {
      props: {
        label: "Menu",
        items: [
          { id: "a", label: "A" },
          { id: "b", label: "B", disabled: true },
        ],
      },
      attachTo: document.body,
    });
    await wrapper.get(".trigger").trigger("click");
    expect(wrapper.get(".trigger").attributes("aria-expanded")).toBe("true");
    await wrapper.findAll('[role="menuitem"]')[1]?.trigger("click");
    expect(wrapper.emitted("select")).toBeUndefined();
    await wrapper.get('[role="menu"]').trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
  });
});

describe("toolbar commands", () => {
  it("registers forwarding variants and the settings entry", () => {
    const forward = vi.fn();
    const openSettings = vi.fn();
    let ids: string[] = [];
    host((registry) => {
        useToolbarCommands(registry, {
          isModalOpen: () => false,
          accounts: ref([]),
          openMessageId: ref(7) as never,
          openSettings,
          reply: vi.fn(),
          replyAll: vi.fn(),
          forward,
          archive: vi.fn(),
          move: vi.fn(),
          copy: vi.fn(),
          junk: vi.fn(),
          remove: vi.fn(),
        });
        ids = registry.commands.value.map((command) => command.id);
        void registry.get("message.forwardAttachment")?.run();
        void registry.get("app.settings")?.run();
    });
    expect(ids).toEqual(
      expect.arrayContaining(["mail.getAll", "app.settings", "message.forwardInline", "message.openWindow"]),
    );
    expect(forward).toHaveBeenCalledWith(7, "attachment");
    expect(openSettings).toHaveBeenCalledOnce();
  });

  it("syncs every account when getting mail", async () => {
    await syncAllAccounts([{ id: 1 }, { id: 2 }] as never);
    expect(vi.mocked(syncAccount).mock.calls.map((call) => call[0])).toEqual([1, 2]);
  });
});

describe("message window routing", () => {
  it("parses only a positive message id", () => {
    expect(messageWindowId("?message=42")).toBe(42);
    expect(messageWindowId("")).toBeNull();
    expect(messageWindowId("?message=-1")).toBeNull();
    expect(messageWindowId("?message=1e3")).toBeNull();
  });
});

describe("HTML viewer", () => {
  it("wraps mail HTML with base styles and zoom", () => {
    expect(framedHtml("<p>x</p>", 1)).not.toContain("zoom");
    expect(framedHtml("<p>x</p>", 1.2)).toContain("html{zoom:1.2}");
    expect(framedHtml("<p>x</p>", 1)).toContain("<p>x</p>");
  });

  it("zooms the frame and remembers the level", async () => {
    const wrapper = mount(MessageBody, {
      props: { textBody: "plain", sanitizedHtml: "<p>Hello</p>" },
    });
    await wrapper.get('button[aria-label="Zoom in"]').trigger("click");
    expect(wrapper.get(".zoom-value").text()).toBe("110%");
    expect(wrapper.get("iframe").attributes("srcdoc")).toContain("zoom:1.1");
    expect(localStorage.getItem("axomail.htmlZoom")).toBe("1.1");
    await wrapper.get(".zoom-value").trigger("click");
    expect(wrapper.get(".zoom-value").text()).toBe("100%");
  });
});
