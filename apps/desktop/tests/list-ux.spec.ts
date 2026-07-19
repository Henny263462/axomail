import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { getListThreaded, setListThreaded } from "../src/api/listPrefs";
import { getMessageDetail, listMessages } from "../src/api/messages";
import type { AccountSummary, MessageListItem } from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  setMessageFlagged: vi.fn(),
  markMailboxRead: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/unified", () => ({ listUnifiedInbox: vi.fn() }));
vi.mock("../src/api/sync", () => ({
  syncAccount: vi.fn(),
  subscribeSyncEvents: vi.fn(),
}));
vi.mock("../src/api/listPrefs", () => ({
  getListThreaded: vi.fn(),
  setListThreaded: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const getListThreadedMock = vi.mocked(getListThreaded);
const setListThreadedMock = vi.mocked(setListThreaded);
const getMessageDetailMock = vi.mocked(getMessageDetail);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
};

function msg(id: number): MessageListItem {
  return {
    id,
    mailboxId: 10,
    subject: `Subject ${id}`,
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: id,
    seen: true,
    flagged: false,
    preview: null,
    tags: [],
  };
}

function page(n: number, from: number): MessageListItem[] {
  return Array.from({ length: n }, (_, i) => msg(from - i));
}

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue([account]);
  getListThreadedMock.mockResolvedValue(false);
  setListThreadedMock.mockResolvedValue(null);
  getMessageDetailMock.mockResolvedValue(null);
});

describe("App infinite scroll", () => {
  it("loads the next page via the stored cursor when the list nears its end", async () => {
    listMessagesMock.mockImplementation((_mailboxId, cursor) =>
      Promise.resolve(
        cursor === null
          ? { items: page(80, 80), nextCursor: "cursor-1" }
          : { items: page(80, 0), nextCursor: null },
      ),
    );

    const wrapper = mount(App);
    await flushPromises();
    await wrapper.get("button.folder-button").trigger("click");
    await flushPromises();

    expect(listMessagesMock).toHaveBeenCalledWith(10, null, 50);

    // Scroll to the bottom of the loaded rows; the window nears the end and the
    // next page is fetched with the stored cursor.
    const box = wrapper.get('[role="listbox"]');
    (box.element as HTMLElement).scrollTop = 80 * 76;
    await box.trigger("scroll");
    await flushPromises();

    expect(listMessagesMock).toHaveBeenCalledWith(10, "cursor-1", 50);
  });
});

describe("App threading toggle", () => {
  it("applies the persisted preference and persists a change", async () => {
    getListThreadedMock.mockResolvedValue(true);
    listMessagesMock.mockResolvedValue({ items: page(3, 3), nextCursor: null });

    const wrapper = mount(App);
    await flushPromises();
    await wrapper.get("button.folder-button").trigger("click");
    await flushPromises();

    const toggle = wrapper
      .findAll("button.chip")
      .find((b) => b.text() === "Threaded");
    expect(toggle!.attributes("aria-pressed")).toBe("true");

    await toggle!.trigger("click");
    await flushPromises();
    expect(setListThreadedMock).toHaveBeenCalledWith(false);
    expect(toggle!.attributes("aria-pressed")).toBe("false");
  });
});
