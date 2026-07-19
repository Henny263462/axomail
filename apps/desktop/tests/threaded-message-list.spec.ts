import { flushPromises, mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it } from "vitest";

import ThreadedMessageList from "../src/components/list/ThreadedMessageList.vue";
import type { MessageListItem } from "../src/types/mail";

function msg(over: Partial<MessageListItem> & { id: number }): MessageListItem {
  return {
    mailboxId: 10,
    subject: `Subject ${over.id}`,
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: over.id,
    seen: true,
    flagged: false,
    preview: null,
    tags: [],
    ...over,
  };
}

/** `n` messages, newest-first by id. */
function many(n: number): MessageListItem[] {
  return Array.from({ length: n }, (_, i) => msg({ id: n - i }));
}

function base(messages: MessageListItem[]) {
  return {
    messages,
    selectedMessageId: null,
    threaded: false,
    listLabel: "Message list",
    emptyDetail: "No messages yet.",
  };
}

describe("ThreadedMessageList virtualization", () => {
  it("renders only the windowed rows for a large list", async () => {
    const wrapper = mount(ThreadedMessageList, { props: base(many(1000)) });
    await nextTick();
    const rendered = wrapper.findAll('[role="option"]').length;
    expect(rendered).toBeGreaterThan(0);
    // Far fewer than the 1000 rows exist in the DOM.
    expect(rendered).toBeLessThan(40);
  });

  it("advances the rendered window as the list scrolls", async () => {
    const wrapper = mount(ThreadedMessageList, { props: base(many(1000)) });
    await nextTick();
    const firstBefore = wrapper.get('[role="option"]').attributes("id");

    const box = wrapper.get('[role="listbox"]');
    (box.element as HTMLElement).scrollTop = 200 * 76;
    await box.trigger("scroll");

    const firstAfter = wrapper.get('[role="option"]').attributes("id");
    expect(firstAfter).not.toBe(firstBefore);
  });
});

describe("ThreadedMessageList infinite scroll", () => {
  it("emits loadMore only once the window nears the loaded end", async () => {
    const wrapper = mount(ThreadedMessageList, {
      props: { ...base(many(100)), hasMore: true },
    });
    await nextTick();
    expect(wrapper.emitted("loadMore")).toBeFalsy();

    const box = wrapper.get('[role="listbox"]');
    (box.element as HTMLElement).scrollTop = 100 * 76;
    await box.trigger("scroll");
    expect(wrapper.emitted("loadMore")).toBeTruthy();
  });

  it("does not ask for more when there is no next page", async () => {
    const wrapper = mount(ThreadedMessageList, {
      props: { ...base(many(100)), hasMore: false },
    });
    const box = wrapper.get('[role="listbox"]');
    (box.element as HTMLElement).scrollTop = 100 * 76;
    await box.trigger("scroll");
    expect(wrapper.emitted("loadMore")).toBeFalsy();
  });
});

describe("ThreadedMessageList accessibility", () => {
  it("exposes a multi-selectable listbox with option rows and an active descendant", async () => {
    const wrapper = mount(ThreadedMessageList, { props: base(many(5)) });
    await nextTick();
    const box = wrapper.get('[role="listbox"]');
    expect(box.attributes("aria-multiselectable")).toBe("true");
    expect(box.attributes("aria-label")).toBe("Message list");
    expect(box.attributes("tabindex")).toBe("0");
    const active = box.attributes("aria-activedescendant");
    expect(active).toBeTruthy();
    // The active descendant is a real rendered option.
    expect(wrapper.find(`#${active}`).attributes("role")).toBe("option");
  });

  it("moves the active row and opens it with the arrow keys", async () => {
    const wrapper = mount(ThreadedMessageList, { props: base(many(5)) });
    await nextTick();
    const box = wrapper.get('[role="listbox"]');
    await box.trigger("keydown", { key: "ArrowDown" });
    // Rows are newest-first [5,4,3,2,1]; ArrowDown moves from index 0 to 1 (id 4).
    expect(wrapper.emitted("select")?.at(-1)).toEqual([4]);
  });
});

describe("ThreadedMessageList quick filter", () => {
  it("filters the rendered rows by the unread facet", async () => {
    const messages = [
      msg({ id: 1, seen: true }),
      msg({ id: 2, seen: false }),
      msg({ id: 3, seen: true }),
    ];
    const wrapper = mount(ThreadedMessageList, { props: base(messages) });
    await nextTick();
    expect(wrapper.findAll('[role="option"]')).toHaveLength(3);

    const unread = wrapper
      .findAll("button.chip")
      .find((b) => b.text() === "Unread");
    await unread!.trigger("click");
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1);
  });
});

describe("ThreadedMessageList threading", () => {
  it("collapses a conversation and expands it via the twisty", async () => {
    const messages = [
      msg({ id: 1, messageId: "a", dateUnix: 100 }),
      msg({ id: 2, messageId: "b", references: ["a"], dateUnix: 200 }),
    ];
    const wrapper = mount(ThreadedMessageList, {
      props: { ...base(messages), threaded: true },
    });
    await nextTick();
    // One collapsed thread row stands in for the two messages.
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1);
    expect(wrapper.get('[role="option"]').attributes("aria-expanded")).toBe(
      "false",
    );

    await wrapper.get("button.twisty").trigger("click");
    // The thread row plus its older reply.
    expect(wrapper.findAll('[role="option"]')).toHaveLength(2);
  });
});

describe("ThreadedMessageList selection", () => {
  it("selects a row via its checkbox and surfaces the selection", async () => {
    const wrapper = mount(ThreadedMessageList, { props: base(many(3)) });
    await nextTick();

    await wrapper.get("input.check").trigger("change");
    await flushPromises();

    expect(wrapper.text()).toContain("1 selected");
    expect(wrapper.get('[role="option"]').attributes("aria-selected")).toBe("true");
    // The resolved message-id selection bubbles up for bulk actions.
    const change = wrapper.emitted("selectionChange")?.at(-1);
    expect(change).toEqual([[3]]);
  });
});
