import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  createTag,
  deleteTag,
  listAllTags,
  listTags,
  messageTagOptions,
  setMessageTag,
} from "../src/api/tags";
import MessageListItem from "../src/components/list/MessageListItem.vue";
import MessageTagArea from "../src/components/message/MessageTagArea.vue";
import TagChips from "../src/components/message/TagChips.vue";
import TagPicker from "../src/components/message/TagPicker.vue";
import TagsSection from "../src/components/settings/TagsSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import SidebarTagsPane from "../src/components/sidebar/SidebarTagsPane.vue";
import type {
  MessageListItem as MessageListItemModel,
  MessageTagOptions,
  TagDto,
} from "../src/types/mail";

vi.mock("../src/api/tags", () => ({
  listTags: vi.fn(),
  listAllTags: vi.fn(),
  createTag: vi.fn(),
  updateTag: vi.fn(),
  deleteTag: vi.fn(),
  messageTagOptions: vi.fn(),
  setMessageTag: vi.fn(),
  listMessagesForTag: vi.fn(),
}));

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const listTagsMock = vi.mocked(listTags);
const listAllTagsMock = vi.mocked(listAllTags);
const createTagMock = vi.mocked(createTag);
const deleteTagMock = vi.mocked(deleteTag);
const messageTagOptionsMock = vi.mocked(messageTagOptions);
const setMessageTagMock = vi.mocked(setMessageTag);

function tag(id: number, name: string, color: string | null, usage = 0): TagDto {
  return {
    id,
    accountId: 1,
    keyword: name,
    displayName: name,
    color,
    sortOrder: id,
    usageCount: usage,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("TagChips", () => {
  it("renders a labelled chip per tag and nothing when empty", () => {
    const wrapper = mount(TagChips, {
      props: {
        tags: [
          { id: 1, displayName: "Work", color: "#e08a3c" },
          { id: 2, displayName: "Later", color: null },
        ],
      },
    });
    const chips = wrapper.findAll(".chip");
    expect(chips).toHaveLength(2);
    expect(wrapper.text()).toContain("Work");
    expect(wrapper.text()).toContain("Later");

    const empty = mount(TagChips, { props: { tags: [] } });
    expect(empty.find("ul").exists()).toBe(false);
  });
});

describe("MessageListItem chips", () => {
  it("renders tag chips for a row that carries tags", () => {
    const message: MessageListItemModel = {
      id: 1,
      mailboxId: 1,
      subject: "Hi",
      fromName: "Ada",
      fromAddress: "ada@example.org",
      dateUnix: 1,
      seen: true,
      flagged: false,
      preview: null,
      tags: [{ id: 5, displayName: "Work", color: "#e08a3c" }],
    };
    const wrapper = mount(MessageListItem, {
      props: { message, selected: false },
    });
    expect(wrapper.find(".chip").exists()).toBe(true);
    expect(wrapper.text()).toContain("Work");
  });
});

describe("TagPicker", () => {
  const options = [
    { id: 1, displayName: "Work", color: "#e08a3c", applied: true },
    { id: 2, displayName: "Later", color: null, applied: false },
  ];

  it("reflects applied state and emits toggle/create/close", async () => {
    const wrapper = mount(TagPicker, { props: { options, busy: false } });
    const boxes = wrapper.findAll('input[type="checkbox"]');
    expect((boxes[0]!.element as HTMLInputElement).checked).toBe(true);
    expect((boxes[1]!.element as HTMLInputElement).checked).toBe(false);

    await boxes[1]!.setValue(true);
    expect(wrapper.emitted("toggle")?.[0]).toEqual([2, true]);

    await wrapper.get('input[type="text"]').setValue("Urgent");
    await wrapper.get("form").trigger("submit");
    expect(wrapper.emitted("create")?.[0]).toEqual(["Urgent"]);

    await wrapper.get('[role="group"]').trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("close")).toBeTruthy();
  });
});

describe("MessageTagArea", () => {
  const options: MessageTagOptions = {
    messageId: 10,
    accountId: 1,
    options: [
      { id: 1, displayName: "Work", color: "#e08a3c", applied: true },
      { id: 2, displayName: "Later", color: null, applied: false },
    ],
  };

  it("shows applied chips and toggles a tag through the picker", async () => {
    messageTagOptionsMock.mockResolvedValue(structuredClone(options));
    setMessageTagMock.mockResolvedValue(null);
    const wrapper = mount(MessageTagArea, { props: { messageId: 10 } });
    await flushPromises();

    // The applied tag shows as a chip.
    expect(wrapper.text()).toContain("Work");

    await wrapper.get('button[aria-haspopup="true"]').trigger("click");
    const picker = wrapper.findComponent(TagPicker);
    expect(picker.exists()).toBe(true);

    await picker.findAll('input[type="checkbox"]')[1]!.setValue(true);
    await flushPromises();
    expect(setMessageTagMock).toHaveBeenCalledWith(10, 2, true);
  });

  it("creates a tag inline, applies it, and reloads", async () => {
    messageTagOptionsMock.mockResolvedValue(structuredClone(options));
    createTagMock.mockResolvedValue(tag(9, "Urgent", null));
    setMessageTagMock.mockResolvedValue(null);
    const wrapper = mount(MessageTagArea, { props: { messageId: 10 } });
    await flushPromises();

    await wrapper.get('button[aria-haspopup="true"]').trigger("click");
    const picker = wrapper.findComponent(TagPicker);
    await picker.get('input[type="text"]').setValue("Urgent");
    await picker.get("form").trigger("submit");
    await flushPromises();

    expect(createTagMock).toHaveBeenCalledWith(1, "Urgent", null);
    expect(setMessageTagMock).toHaveBeenCalledWith(10, 9, true);
  });
});

describe("SidebarTagsPane", () => {
  it("lists tags and emits selectTag on click", async () => {
    listAllTagsMock.mockResolvedValue([
      tag(1, "Work", "#e08a3c"),
      tag(2, "Later", null),
    ]);
    const wrapper = mount(SidebarTagsPane);
    await flushPromises();

    const buttons = wrapper.findAll(".tag");
    expect(buttons).toHaveLength(2);
    await buttons[0]!.trigger("click");
    expect(wrapper.emitted("selectTag")?.[0]).toEqual([1, "Work"]);
  });

  it("renders nothing when there are no tags", async () => {
    listAllTagsMock.mockResolvedValue([]);
    const wrapper = mount(SidebarTagsPane);
    await flushPromises();
    expect(wrapper.find(".tags-group").exists()).toBe(false);
  });
});

describe("TagsSection", () => {
  function stubContext(): SettingsSectionContext {
    return {
      accounts: [
        {
          id: 1,
          displayName: "Work",
          email: "work@example.org",
          authMechanism: "password",
          mailboxes: [],
        },
      ],
      initialPolicies: [],
      notificationsEnabled: false,
      syncIntervalMinutes: 5,
      theme: "system",
      autoMarkReadEnabled: true,
      autoMarkReadDelayMs: 0,
      setNotificationsEnabled: vi.fn(),
      setSyncIntervalMinutes: vi.fn(),
      setTheme: vi.fn(),
      setAutoMarkRead: vi.fn(),
      reportError: vi.fn(),
      reportStatus: vi.fn(),
      notifyAccountsChanged: vi.fn(),
    };
  }

  it("lists tags and requires confirmation before deleting one in use", async () => {
    listTagsMock.mockResolvedValue([tag(1, "Work", "#e08a3c", 3)]);
    deleteTagMock.mockResolvedValue(null);
    const wrapper = mount(TagsSection, { props: { context: stubContext() } });
    await flushPromises();

    expect((wrapper.get("input.name").element as HTMLInputElement).value).toBe(
      "Work",
    );
    // First Delete click on an in-use tag asks to confirm, not delete.
    await wrapper.get("button.danger").trigger("click");
    expect(deleteTagMock).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("3 message");

    // Confirming deletes it.
    await wrapper.get("button.danger").trigger("click");
    await flushPromises();
    expect(deleteTagMock).toHaveBeenCalledWith(1);
  });

  it("creates a tag from the inline form", async () => {
    listTagsMock.mockResolvedValue([]);
    createTagMock.mockResolvedValue(tag(2, "Project", "#3c78d8"));
    const context = stubContext();
    const wrapper = mount(TagsSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[aria-label="New tag name"]').setValue("Project");
    await wrapper.get("form.create").trigger("submit");
    await flushPromises();

    expect(createTagMock).toHaveBeenCalledWith(1, "Project", expect.any(String));
    expect(context.reportStatus).toHaveBeenCalledWith(expect.stringContaining("Project"));
  });
});
