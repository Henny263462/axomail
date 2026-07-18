import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { listMessages } from "../src/api/messages";
import { subscribeSyncEvents, syncAccount } from "../src/api/sync";
import type { AccountSummary, SyncEventDto } from "../src/types/mail";

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

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const syncAccountMock = vi.mocked(syncAccount);
const subscribeSyncEventsMock = vi.mocked(subscribeSyncEvents);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 1 }],
};

let syncHandler: ((event: SyncEventDto) => void) | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  syncHandler = null;
  listAccountsMock.mockResolvedValue([account]);
  listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });
  syncAccountMock.mockResolvedValue([5]);
  subscribeSyncEventsMock.mockImplementation((handler) => {
    syncHandler = handler;
    return Promise.resolve(() => {});
  });
});

describe("App sync wiring", () => {
  it("syncs an account and reloads the visible mailbox on batchIngested", async () => {
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.mailbox").trigger("click");
    await flushPromises();
    expect(listMessagesMock).toHaveBeenCalledTimes(1);

    await wrapper.get("button.sync").trigger("click");
    await flushPromises();
    expect(syncAccountMock).toHaveBeenCalledWith(1);

    if (syncHandler === null) throw new Error("sync handler not registered");
    syncHandler({
      kind: "batchIngested",
      jobId: 5,
      mailboxId: 10,
      newMessages: 2,
      errorClass: null,
      message: null,
    });
    await flushPromises();

    expect(listMessagesMock).toHaveBeenCalledTimes(2);
    expect(listMessagesMock).toHaveBeenLastCalledWith(10, null, 50);
  });

  it("surfaces a job failure in the status bar and dismisses it", async () => {
    const wrapper = mount(App);
    await flushPromises();

    if (syncHandler === null) throw new Error("sync handler not registered");
    syncHandler({
      kind: "jobFailed",
      jobId: 9,
      mailboxId: 10,
      newMessages: null,
      errorClass: "network",
      message: "Sync failed: connection timed out.",
    });
    await flushPromises();

    const status = wrapper.get('[aria-label="Status"]');
    expect(status.text()).toContain("Sync failed: connection timed out.");

    await status.get("button.dismiss").trigger("click");
    expect(wrapper.get('[aria-label="Status"]').text()).not.toContain(
      "Sync failed: connection timed out.",
    );
  });
});
