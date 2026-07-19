import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { getMessageDetail, listMessages } from "../src/api/messages";
import { listUnifiedInbox } from "../src/api/unified";
import type {
  AccountSummary,
  MessageDetailDto,
  UnifiedItemDto,
} from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/sync", () => ({
  syncAccount: vi.fn(),
  subscribeSyncEvents: vi.fn(),
}));
vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
  listDrafts: vi.fn(),
  deleteDraft: vi.fn(),
  listOutbox: vi.fn(),
  retryOutbox: vi.fn(),
}));
vi.mock("../src/api/unified", () => ({
  listUnifiedInbox: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const listUnifiedInboxMock = vi.mocked(listUnifiedInbox);
const getMessageDetailMock = vi.mocked(getMessageDetail);

const accounts: AccountSummary[] = [
  {
    id: 1,
    displayName: "Ferber Mail",
    email: "klaus@theferbers.eu",
    mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 1 }],
  },
  {
    id: 2,
    displayName: "Legacy POP",
    email: "work@example.org",
    mailboxes: [{ id: 20, name: "Inbox", role: "inbox", unreadCount: 0 }],
  },
];

const unifiedItems: UnifiedItemDto[] = [
  {
    id: 202,
    mailboxId: 20,
    accountId: 2,
    subject: "Work note",
    fromName: "Bob",
    fromAddress: "bob@example.org",
    dateUnix: 1_752_100_000,
    seen: true,
    flagged: false,
    preview: "note",
  },
  {
    id: 101,
    mailboxId: 10,
    accountId: 1,
    subject: "Hello from Ferber",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1_752_000_000,
    seen: false,
    flagged: false,
    preview: "hi",
  },
];

const detail: MessageDetailDto = {
  id: 202,
  subject: "Work note",
  fromName: "Bob",
  fromAddress: "bob@example.org",
  to: [],
  cc: [],
  dateUnix: 1_752_100_000,
  textBody: "note body",
  sanitizedHtml: null,
  remoteBlocked: 0,
  hadActiveContent: false,
  attachments: [],
  messageId: "work-note-1@example.org",
  inReplyTo: [],
  references: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue(accounts);
  listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });
  listUnifiedInboxMock.mockResolvedValue({
    items: unifiedItems,
    nextCursor: null,
  });
  getMessageDetailMock.mockResolvedValue(detail);
});

describe("App unified inbox", () => {
  it("renders the fixed All inboxes entry in the sidebar", async () => {
    const wrapper = mount(App);
    await flushPromises();

    expect(
      wrapper.get('[aria-label="Accounts and folders"]').text(),
    ).toContain("All inboxes");
  });

  it("loads the unified inbox on select and shows account names", async () => {
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.all-inboxes").trigger("click");
    await flushPromises();

    expect(listUnifiedInboxMock).toHaveBeenCalledWith(null, 50);
    const list = wrapper.get('[aria-label="Unified inbox"]');
    expect(list.text()).toContain("Hello from Ferber");
    expect(list.text()).toContain("Work note");
    // Each item is tagged with its account's display name.
    expect(list.text()).toContain("Ferber Mail");
    expect(list.text()).toContain("Legacy POP");
  });

  it("opens a message's detail when a unified item is selected", async () => {
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.all-inboxes").trigger("click");
    await flushPromises();

    // The first rendered item (id 202) carries a real message id.
    await wrapper.get("button.item").trigger("click");
    await flushPromises();

    expect(getMessageDetailMock).toHaveBeenCalledWith(202);
    expect(wrapper.get('[aria-label="Message"]').text()).toContain("Work note");
  });
});
