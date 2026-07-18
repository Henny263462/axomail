import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "../src/App.vue";
import { createAccount, listAccounts } from "../src/api/accounts";
import { listMessages, markMessageSeen } from "../src/api/messages";
import type { AccountSummary } from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));
vi.mock("../src/api/messages", () => ({
  listMessages: vi.fn(),
  markMessageSeen: vi.fn(),
}));

const listAccountsMock = vi.mocked(listAccounts);
const createAccountMock = vi.mocked(createAccount);
const listMessagesMock = vi.mocked(listMessages);
const markMessageSeenMock = vi.mocked(markMessageSeen);

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  mailboxes: [
    { id: 10, name: "Inbox", role: "inbox", unreadCount: 3 },
    { id: 11, name: "Sent", role: "sent", unreadCount: 0 },
  ],
};

async function fillDialog(wrapper: VueWrapper): Promise<void> {
  await wrapper.get("#account-display-name").setValue("Klaus Ferber");
  await wrapper.get("#account-email").setValue("klaus@theferbers.eu");
  await wrapper.get("#incoming-host").setValue("imap.theferbers.eu");
  await wrapper.get("#incoming-username").setValue("klaus");
  await wrapper.get("#incoming-password").setValue("incoming-secret");
  await wrapper.get("#outgoing-host").setValue("smtp.theferbers.eu");
  await wrapper.get("#outgoing-username").setValue("klaus");
  await wrapper.get("#outgoing-password").setValue("outgoing-secret");
}

beforeEach(() => {
  vi.clearAllMocks();
  listAccountsMock.mockResolvedValue([]);
  listMessagesMock.mockResolvedValue({ items: [], nextCursor: null });
  markMessageSeenMock.mockResolvedValue(null);
  createAccountMock.mockResolvedValue(account);
});

describe("App account wiring", () => {
  it("loads accounts on mount and renders them", async () => {
    listAccountsMock.mockResolvedValue([account]);
    const wrapper = mount(App);
    await flushPromises();

    expect(listAccountsMock).toHaveBeenCalledTimes(1);
    const sidebar = wrapper.get('[aria-label="Accounts and folders"]');
    expect(sidebar.text()).toContain("Ferber Mail");
    expect(sidebar.text()).toContain("Inbox");
  });

  it("loads the first message page when a mailbox is selected", async () => {
    listAccountsMock.mockResolvedValue([account]);
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.mailbox").trigger("click");
    await flushPromises();

    expect(listMessagesMock).toHaveBeenCalledWith(10, null, 50);
  });

  it("opens the setup dialog from the sidebar button", async () => {
    const wrapper = mount(App);
    await flushPromises();

    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    await wrapper.get("button.add-account").trigger("click");

    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.attributes("aria-modal")).toBe("true");
    expect(dialog.attributes("aria-labelledby")).toBe("account-setup-title");
  });

  it("creates an account, closes the dialog, and refreshes accounts", async () => {
    listAccountsMock.mockResolvedValueOnce([]);
    listAccountsMock.mockResolvedValueOnce([account]);
    const wrapper = mount(App);
    await flushPromises();

    await wrapper.get("button.add-account").trigger("click");
    await fillDialog(wrapper);
    await wrapper.get('[role="dialog"] form').trigger("submit");
    await flushPromises();

    expect(createAccountMock).toHaveBeenCalledTimes(1);
    expect(listAccountsMock).toHaveBeenCalledTimes(2);
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.get('[aria-label="Accounts and folders"]').text()).toContain(
      "Ferber Mail",
    );
    // The new account's inbox is selected, so its first page is loaded.
    expect(listMessagesMock).toHaveBeenCalledWith(10, null, 50);
  });
});
