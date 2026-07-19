import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { listAccounts } from "../src/api/accounts";
import { listMessages, markMessageSeen } from "../src/api/messages";
import { listUnifiedInbox } from "../src/api/unified";
import DialogHost from "../src/components/layout/DialogHost.vue";
import type { AccountSummary } from "../src/types/mail";

// App reaches the accounts/messages/unified apis on mount and while
// reconciling; stub just those so the shell mounts without a backend.
vi.mock("../src/api/accounts", () => ({ listAccounts: vi.fn() }));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
  getMessageDetail: vi.fn(),
}));
vi.mock("../src/api/unified", () => ({ listUnifiedInbox: vi.fn() }));

const listAccountsMock = vi.mocked(listAccounts);
const listMessagesMock = vi.mocked(listMessages);
const markMessageSeenMock = vi.mocked(markMessageSeen);
const listUnifiedInboxMock = vi.mocked(listUnifiedInbox);

/** An account with one inbox mailbox, at a chosen display name. */
function account(displayName: string): AccountSummary {
  return {
    id: 1,
    displayName,
    email: "klaus@theferbers.eu",
    authMechanism: "password",
    mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 2 }],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });
  markMessageSeenMock.mockResolvedValue(null);
  listUnifiedInboxMock.mockResolvedValue({ items: [], nextCursor: null });
});

describe("App account-change propagation", () => {
  it("refreshes accounts and reconciles selection when an account is deleted", async () => {
    // The first load shows the account; after deletion it is gone.
    listAccountsMock.mockResolvedValueOnce([account("Ferber Mail")]);
    listAccountsMock.mockResolvedValueOnce([]);
    const wrapper = mount(App);
    await flushPromises();

    // Select the account's inbox so there is a live selection to reconcile.
    await wrapper.get("button.folder-button").trigger("click");
    await flushPromises();
    expect(listMessagesMock).toHaveBeenCalledWith(10, null, 50);

    // Settings deleted the account and bubbled the signal to the shell.
    wrapper.findComponent(DialogHost).vm.$emit("accountsChanged");
    await flushPromises();

    // Accounts were re-fetched (mount + refresh) and the sidebar lost the account.
    expect(listAccountsMock).toHaveBeenCalledTimes(2);
    const sidebar = wrapper.get('[aria-label="Accounts and folders"]');
    expect(sidebar.text()).not.toContain("Ferber Mail");
    // The now-dangling mailbox selection fell back cleanly to the unified inbox.
    expect(listUnifiedInboxMock).toHaveBeenCalled();
    expect(sidebar.get("button.all-inboxes").classes()).toContain("selected");
  });

  it("reflects a renamed account in the sidebar without dropping the selection", async () => {
    listAccountsMock.mockResolvedValueOnce([account("Ferber Mail")]);
    listAccountsMock.mockResolvedValueOnce([account("Renamed Mail")]);
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.folder-button").trigger("click");
    await flushPromises();

    wrapper.findComponent(DialogHost).vm.$emit("accountsChanged");
    await flushPromises();

    const sidebar = wrapper.get('[aria-label="Accounts and folders"]');
    expect(sidebar.text()).toContain("Renamed Mail");
    expect(sidebar.text()).not.toContain("Ferber Mail");
    // The mailbox still exists, so the selection is preserved (no reconcile).
    expect(listUnifiedInboxMock).not.toHaveBeenCalled();
    expect(wrapper.get("button.folder-button").classes()).toContain("selected");
  });
});
