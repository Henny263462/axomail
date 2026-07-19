import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  deleteAccount,
  listAccountDetails,
  updateAccount,
} from "../src/api/accounts";
import { testConnection } from "../src/api/connection";
import AccountsSection from "../src/components/settings/AccountsSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import type { AccountDetail } from "../src/types/mail";

// AccountsSection is a self-contained registry section: it ignores the shared
// context, but the registry contract requires the prop, so tests supply a stub.
const context = {
  accounts: [],
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
  notifyTagsChanged: vi.fn(),
} satisfies SettingsSectionContext;

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  listAccountDetails: vi.fn(),
  createAccount: vi.fn(),
  updateAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));

vi.mock("../src/api/connection", () => ({
  testConnection: vi.fn(),
}));

const listAccountDetailsMock = vi.mocked(listAccountDetails);
const updateAccountMock = vi.mocked(updateAccount);
const deleteAccountMock = vi.mocked(deleteAccount);
const testConnectionMock = vi.mocked(testConnection);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

function sampleDetails(): AccountDetail[] {
  return [
    {
      id: 1,
      displayName: "Ferber Mail",
      email: "klaus@theferbers.eu",
      incoming: {
        protocol: "imap",
        host: "imap.theferbers.eu",
        port: 993,
        security: "tls",
        username: "klaus",
      },
      outgoing: {
        host: "smtp.theferbers.eu",
        port: 465,
        security: "tls",
        username: "klaus",
      },
    },
    {
      id: 2,
      displayName: "Legacy POP",
      email: "work@example.org",
      incoming: {
        protocol: "pop3",
        host: "pop.example.org",
        port: 995,
        security: "tls",
        username: "work",
      },
      outgoing: {
        host: "smtp.example.org",
        port: 587,
        security: "starttls",
        username: "work",
      },
    },
  ];
}

async function mountReady(): Promise<VueWrapper> {
  listAccountDetailsMock.mockResolvedValue(sampleDetails());
  const wrapper = mount(AccountsSection, {
    props: { context },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  vi.clearAllMocks();
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  updateAccountMock.mockResolvedValue(null);
  deleteAccountMock.mockResolvedValue(null);
  testConnectionMock.mockResolvedValue({
    incomingOk: true,
    incomingError: null,
    outgoingOk: true,
    outgoingError: null,
  });
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("AccountsSection list", () => {
  it("lists each account with name, email, and protocols", async () => {
    const wrapper = await mountReady();

    expect(listAccountDetailsMock).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).toContain("Ferber Mail");
    expect(wrapper.text()).toContain("klaus@theferbers.eu");
    expect(wrapper.text()).toContain("IMAP · SMTP");
    expect(wrapper.text()).toContain("Legacy POP");
    expect(wrapper.text()).toContain("POP3 · SMTP");
    wrapper.unmount();
  });

  it("shows an honest empty state when there are no accounts", async () => {
    listAccountDetailsMock.mockResolvedValue([]);
    const wrapper = mount(AccountsSection, { props: { context } });
    await flushPromises();

    expect(wrapper.text()).toContain("No accounts yet");
    expect(wrapper.find("ul").exists()).toBe(false);
  });

  it("explains that management needs the desktop app in the browser preview", async () => {
    listAccountDetailsMock.mockResolvedValue(null);
    const wrapper = mount(AccountsSection, { props: { context } });
    await flushPromises();

    expect(wrapper.get('[role="status"]').text()).toContain(
      "needs the desktop app",
    );
  });
});

describe("AccountsSection edit", () => {
  it("saves edited server fields through updateAccount and reloads", async () => {
    const wrapper = await mountReady();

    await wrapper.findAll("button").find((b) => b.text() === "Edit")!.trigger(
      "click",
    );
    await flushPromises();

    await wrapper.get("#edit-display-name").setValue("Renamed");
    await wrapper.get("#edit-incoming-host").setValue("imap2.theferbers.eu");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(updateAccountMock).toHaveBeenCalledWith({
      accountId: 1,
      displayName: "Renamed",
      incoming: { host: "imap2.theferbers.eu", port: 993, security: "tls" },
      outgoing: { host: "smtp.theferbers.eu", port: 465, security: "tls" },
    });
    // Mount load + reload after a successful save.
    expect(listAccountDetailsMock).toHaveBeenCalledTimes(2);
    // The shell is signalled so the sidebar reflects the rename too.
    expect(context.notifyAccountsChanged).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("blocks saving and shows an error when the display name is empty", async () => {
    const wrapper = await mountReady();

    await wrapper.findAll("button").find((b) => b.text() === "Edit")!.trigger(
      "click",
    );
    await flushPromises();

    await wrapper.get("#edit-display-name").setValue("");
    const save = wrapper.get('button[type="submit"]');
    expect((save.element as HTMLButtonElement).disabled).toBe(true);
    expect(wrapper.text()).toContain("Display name is required.");
    await wrapper.get("form").trigger("submit");
    expect(updateAccountMock).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("reuses test_connection with the edited server fields", async () => {
    const wrapper = await mountReady();

    await wrapper.findAll("button").find((b) => b.text() === "Edit")!.trigger(
      "click",
    );
    await flushPromises();

    await wrapper.get("#edit-incoming-host").setValue("imap2.theferbers.eu");
    await wrapper.get(".test .link").trigger("click");
    await wrapper.get("#edit-incoming-password").setValue("secret-in");
    await wrapper.get("#edit-outgoing-password").setValue("secret-out");
    await wrapper.findAll("button").find((b) => b.text() === "Run test")!
      .trigger("click");
    await flushPromises();

    expect(testConnectionMock).toHaveBeenCalledTimes(1);
    const input = testConnectionMock.mock.calls[0]![0];
    expect(input.incoming.host).toBe("imap2.theferbers.eu");
    expect(input.incoming.protocol).toBe("imap");
    expect(input.incoming.password).toBe("secret-in");
    expect(input.outgoing.password).toBe("secret-out");
    wrapper.unmount();
  });
});

describe("AccountsSection delete", () => {
  async function openDeleteDialog(wrapper: VueWrapper): Promise<void> {
    await wrapper.findAll("button").find((b) => b.text() === "Delete")!.trigger(
      "click",
    );
    await flushPromises();
  }

  it("gates the delete button behind typing the exact account email", async () => {
    const wrapper = await mountReady();
    await openDeleteDialog(wrapper);

    const dialog = wrapper.get('[role="alertdialog"]');
    const confirm = dialog.get("button.danger");
    expect((confirm.element as HTMLButtonElement).disabled).toBe(true);

    // A near-miss keeps the button disabled.
    await dialog.get("#delete-account-confirm").setValue("klaus@theferbers.e");
    expect((confirm.element as HTMLButtonElement).disabled).toBe(true);
    expect(deleteAccountMock).not.toHaveBeenCalled();

    // The exact address unlocks it.
    await dialog.get("#delete-account-confirm").setValue("klaus@theferbers.eu");
    expect((confirm.element as HTMLButtonElement).disabled).toBe(false);
    wrapper.unmount();
  });

  it("deletes through deleteAccount and reloads once confirmed", async () => {
    const wrapper = await mountReady();
    await openDeleteDialog(wrapper);

    const dialog = wrapper.get('[role="alertdialog"]');
    await dialog.get("#delete-account-confirm").setValue("klaus@theferbers.eu");
    await dialog.get("button.danger").trigger("click");
    await flushPromises();

    expect(deleteAccountMock).toHaveBeenCalledWith(1);
    expect(listAccountDetailsMock).toHaveBeenCalledTimes(2);
    expect(wrapper.find('[role="alertdialog"]').exists()).toBe(false);
    // The removed account must be reconciled out of the app shell too.
    expect(context.notifyAccountsChanged).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("keeps the dialog open and surfaces a delete failure", async () => {
    deleteAccountMock.mockRejectedValue("The account is in use.");
    const wrapper = await mountReady();
    await openDeleteDialog(wrapper);

    const dialog = wrapper.get('[role="alertdialog"]');
    await dialog.get("#delete-account-confirm").setValue("klaus@theferbers.eu");
    await dialog.get("button.danger").trigger("click");
    await flushPromises();

    expect(wrapper.get('[role="alertdialog"] [role="alert"]').text()).toContain(
      "The account is in use.",
    );
    // The account was not removed from the list, so no reload happened.
    expect(listAccountDetailsMock).toHaveBeenCalledTimes(1);
    // A failed delete must not signal the shell to reconcile.
    expect(context.notifyAccountsChanged).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
