import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import EmptyState from "../src/components/common/EmptyState.vue";
import MessageListPane from "../src/components/list/MessageListPane.vue";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import SidebarPane from "../src/components/sidebar/SidebarPane.vue";
import { fixtureAccounts, fixtureMessages } from "../src/dev/fixtures";

describe("EmptyState", () => {
  it("renders title and detail", () => {
    const wrapper = mount(EmptyState, {
      props: { title: "Nothing here", detail: "Come back later." },
    });
    expect(wrapper.text()).toContain("Nothing here");
    expect(wrapper.text()).toContain("Come back later.");
  });
});

describe("SidebarPane", () => {
  it("lists mailboxes with unread counts and emits selection", async () => {
    const wrapper = mount(SidebarPane, {
      props: { accounts: fixtureAccounts, selectedMailboxId: null },
    });
    expect(wrapper.text()).toContain("INBOX");
    expect(wrapper.text()).toContain("2");

    await wrapper.get("button.folder-button").trigger("click");
    expect(wrapper.emitted("selectMailbox")?.[0]).toEqual([1]);
  });
});

describe("MessageListPane", () => {
  it("renders envelopes and emits the selected message id", async () => {
    const wrapper = mount(MessageListPane, {
      props: { messages: fixtureMessages, selectedMessageId: null },
    });
    expect(wrapper.text()).toContain("Design tokens review");

    await wrapper.get("button.item").trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual([101]);
  });

  it("marks unseen messages", () => {
    const wrapper = mount(MessageListPane, {
      props: { messages: fixtureMessages, selectedMessageId: null },
    });
    expect(wrapper.findAll("button.item.unseen").length).toBe(2);
  });
});

describe("MessageViewPane", () => {
  it("shows an empty state without a selection", () => {
    const wrapper = mount(MessageViewPane, { props: { messageId: null } });
    expect(wrapper.text()).toContain("No message selected");
  });
});
