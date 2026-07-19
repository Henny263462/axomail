import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { getMessageDetail, listMessages } from "../src/api/messages";
import { getListThreaded } from "../src/api/listPrefs";
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
  markMessagesRead: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/listPrefs", () => ({
  getListThreaded: vi.fn(),
  setListThreaded: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const getMessageDetailMock = vi.mocked(getMessageDetail);
const getListThreadedMock = vi.mocked(getListThreaded);

function mailbox(over: Partial<AccountSummary["mailboxes"][number]> & { id: number }) {
  return {
    name: "Folder",
    role: "custom" as const,
    unreadCount: 0,
    totalCount: 0,
    parentId: null,
    delimiter: "/",
    subscribed: true,
    orphaned: false,
    ...over,
  };
}

const account: AccountSummary = {
  id: 1,
  displayName: "Work",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [
    mailbox({ id: 10, name: "Inbox", role: "inbox" }),
    mailbox({ id: 12, name: "Trash", role: "trash", totalCount: 2 }),
  ],
};

function msg(id: number): MessageListItem {
  return {
    id,
    mailboxId: 12,
    subject: `Trashed ${id}`,
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: id,
    seen: true,
    flagged: false,
    preview: null,
    tags: [],
  };
}

async function selectTrash(wrapper: VueWrapper): Promise<void> {
  const trash = wrapper
    .findAll("button.folder-button")
    .find((b) => b.text().includes("Trash"));
  if (trash === undefined) throw new Error("Trash folder button missing");
  await trash.trigger("click");
  await flushPromises();
}

async function emptyTrashViaMenu(wrapper: VueWrapper): Promise<void> {
  const row = wrapper
    .findAll(".folder-row")
    .find((r) => r.find(".folder-name").text() === "Trash");
  if (row === undefined) throw new Error("Trash row missing");
  await row.get(".folder-menu").trigger("click");

  const item = wrapper
    .findAll('[role="menuitem"]')
    .find((b) => b.text() === "Empty Trash…");
  await item?.trigger("click");

  const dialog = wrapper.get('[role="alertdialog"]');
  await dialog.get("input").setValue("empty");
  const confirm = dialog.findAll("button").find((b) => b.text() === "Empty Trash");
  if (confirm === undefined) throw new Error("confirm button missing");
  await confirm.trigger("click");
  await flushPromises();
}

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue([account]);
  getListThreadedMock.mockResolvedValue(false);
  getMessageDetailMock.mockResolvedValue(null);
});

describe("Empty Trash reloads the open list (finding 6)", () => {
  it("reloads the Trash view so permanently-deleted messages disappear", async () => {
    // First load shows the trashed message; the post-empty reload returns empty.
    listMessagesMock.mockResolvedValueOnce({ items: [msg(1)], nextCursor: null });
    listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });

    const wrapper = mount(App, { attachTo: document.body });
    await flushPromises();

    await selectTrash(wrapper);
    expect(listMessagesMock).toHaveBeenCalledWith(12, null, 50);
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1);

    await emptyTrashViaMenu(wrapper);

    // The list was reloaded and now shows the empty state.
    expect(listMessagesMock).toHaveBeenCalledTimes(2);
    expect(wrapper.findAll('[role="option"]')).toHaveLength(0);
    expect(wrapper.get('[aria-label="Message list"]').text()).toContain(
      "No messages",
    );
    wrapper.unmount();
  });

  it("drops the open detail when the reloaded list comes back empty", async () => {
    listMessagesMock.mockResolvedValueOnce({ items: [msg(1)], nextCursor: null });
    listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });

    const wrapper = mount(App, { attachTo: document.body });
    await flushPromises();
    await selectTrash(wrapper);

    // Open the trashed message.
    await wrapper.get('[role="option"] button.item').trigger("click");
    await flushPromises();

    await emptyTrashViaMenu(wrapper);

    // With the message purged, the reading pane falls back to its idle state.
    expect(wrapper.get('[aria-label="Message"]').text()).toContain(
      "No message selected",
    );
    wrapper.unmount();
  });
});
