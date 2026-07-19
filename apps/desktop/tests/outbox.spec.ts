import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { listOutbox, retryOutbox } from "../src/api/compose";
import type { AccountSummary, OutboxItemDto } from "../src/types/mail";

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
  subscribeSyncEvents: vi.fn().mockResolvedValue(() => {}),
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
const listOutboxMock = vi.mocked(listOutbox);
const retryOutboxMock = vi.mocked(retryOutbox);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
};

const failedItem: OutboxItemDto = {
  id: 9,
  accountId: 1,
  subject: "Stuck message",
  toSummary: "ops@example.org",
  state: "failed",
  attempts: 2,
  lastError: "Connection refused",
  updatedUnix: 1_752_790_000,
};

function outboxButton(wrapper: ReturnType<typeof mount>) {
  const button = wrapper
    .findAll("button.special")
    .find((candidate) => candidate.text() === "Outbox");
  if (button === undefined) throw new Error("outbox entry missing");
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue([account]);
  listOutboxMock.mockResolvedValueOnce([failedItem]);
  listOutboxMock.mockResolvedValueOnce([]);
  retryOutboxMock.mockResolvedValue({ outboxId: 9, sent: true, error: null });
});

describe("App outbox wiring", () => {
  it("lists outbox items and retries a failed one, then refreshes", async () => {
    const wrapper = mount(App);
    await flushPromises();

    await outboxButton(wrapper).trigger("click");
    await flushPromises();

    expect(listOutboxMock).toHaveBeenCalledTimes(1);
    const outbox = wrapper.get('[aria-label="Outbox"]');
    expect(outbox.text()).toContain("Stuck message");
    expect(outbox.text()).toContain("Failed");
    expect(outbox.text()).toContain("Connection refused");
    expect(outbox.text()).toContain("2 attempts");

    await wrapper.get("button.retry").trigger("click");
    await flushPromises();

    expect(retryOutboxMock).toHaveBeenCalledWith(9);
    expect(listOutboxMock).toHaveBeenCalledTimes(2);
  });
});
