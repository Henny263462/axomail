import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import {
  listMessages,
  markMessagesRead,
  setMessageFlagged,
} from "../src/api/messages";
import { listAllTags, listMessagesForTag } from "../src/api/tags";
import { listUnifiedInbox } from "../src/api/unified";
import type {
  AccountSummary,
  MessageListItem,
  TagDto,
} from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({ listAccounts: vi.fn() }));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  setMessageFlagged: vi.fn(),
  markMailboxRead: vi.fn(),
  markMessagesRead: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/unified", () => ({ listUnifiedInbox: vi.fn() }));
vi.mock("../src/api/tags", () => ({
  listAllTags: vi.fn(),
  listMessagesForTag: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const setMessageFlaggedMock = vi.mocked(setMessageFlagged);
const markMessagesReadMock = vi.mocked(markMessagesRead);
const listAllTagsMock = vi.mocked(listAllTags);
const listMessagesForTagMock = vi.mocked(listMessagesForTag);
const listUnifiedInboxMock = vi.mocked(listUnifiedInbox);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 1 }],
};

function tagMessage(over: Partial<MessageListItem> = {}): MessageListItem {
  return {
    id: 1,
    mailboxId: 10,
    subject: "Tagged",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1,
    seen: false,
    flagged: false,
    preview: null,
    tags: [{ id: 7, displayName: "Work", color: null }],
    ...over,
  };
}

function tag(id: number, name: string): TagDto {
  return {
    id,
    accountId: 1,
    keyword: name,
    displayName: name,
    color: null,
    sortOrder: id,
    usageCount: 1,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue([account]);
  listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });
  listUnifiedInboxMock.mockResolvedValue({ items: [], nextCursor: null });
  listAllTagsMock.mockResolvedValue([tag(7, "Work")]);
  listMessagesForTagMock.mockResolvedValue({
    items: [tagMessage()],
    nextCursor: null,
  });
  setMessageFlaggedMock.mockResolvedValue(null);
  markMessagesReadMock.mockResolvedValue(null);
});

/** Opens the sidebar Tags group's only tag, entering the tag-filter view. */
async function openTagView(wrapper: VueWrapper): Promise<void> {
  await wrapper.get(".tag").trigger("click");
  await flushPromises();
}

describe("App tag-filter view", () => {
  it("stars a message from the tag list (toggle-flag wired)", async () => {
    const wrapper = mount(App);
    await flushPromises();
    await openTagView(wrapper);
    expect(listMessagesForTagMock).toHaveBeenCalledWith(7);

    await wrapper.get("button.star").trigger("click");
    await flushPromises();
    expect(setMessageFlaggedMock).toHaveBeenCalledWith(1, true);
    wrapper.unmount();
  });

  it("marks all read in the tag list (mark-all-read wired)", async () => {
    const wrapper = mount(App);
    await flushPromises();
    await openTagView(wrapper);

    await wrapper.get("button.mark-read").trigger("click");
    await flushPromises();
    expect(markMessagesReadMock).toHaveBeenCalledWith([1], true);
    wrapper.unmount();
  });
});
