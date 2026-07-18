import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createAccount } from "../src/api/accounts";
import AccountSetupDialog from "../src/components/setup/AccountSetupDialog.vue";
import SidebarPane from "../src/components/sidebar/SidebarPane.vue";
import type { AccountSummary } from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));

const createAccountMock = vi.mocked(createAccount);

const createdAccount: AccountSummary = {
  id: 7,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  mailboxes: [{ id: 70, name: "Inbox", role: "inbox", unreadCount: 0 }],
};

async function fillRequiredFields(
  wrapper: VueWrapper,
): Promise<void> {
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
  createAccountMock.mockResolvedValue(createdAccount);
});

describe("SidebarPane add-account button", () => {
  it("is present with the updated empty-state guidance", () => {
    const wrapper = mount(SidebarPane, {
      props: { accounts: [], selectedMailboxId: null },
    });
    expect(wrapper.get("button.add-account").text()).toBe("Add account");
    expect(wrapper.text()).toContain("Add an account to get started.");
  });

  it("emits addAccount when clicked", async () => {
    const wrapper = mount(SidebarPane, {
      props: { accounts: [], selectedMailboxId: null },
    });
    await wrapper.get("button.add-account").trigger("click");
    expect(wrapper.emitted("addAccount")).toBeTruthy();
  });
});

describe("AccountSetupDialog", () => {
  it("moves focus into the dialog on open", () => {
    const wrapper = mount(AccountSetupDialog, { attachTo: document.body });
    expect(document.activeElement).toBe(
      wrapper.get("#account-display-name").element,
    );
    wrapper.unmount();
  });

  it("autofills ports when protocol and security change", async () => {
    const wrapper = mount(AccountSetupDialog);

    const port = (id: string) =>
      (wrapper.get(id).element as HTMLInputElement).value;

    expect(port("#incoming-port")).toBe("993");
    expect(port("#outgoing-port")).toBe("465");

    await wrapper.get("#incoming-protocol").setValue("pop3");
    expect(port("#incoming-port")).toBe("995");

    await wrapper.get("#incoming-security").setValue("starttls");
    expect(port("#incoming-port")).toBe("110");

    await wrapper.get("#outgoing-security").setValue("starttls");
    expect(port("#outgoing-port")).toBe("587");
  });

  it("does not override a port the user edited by hand", async () => {
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("#incoming-port").setValue("1993");
    await wrapper.get("#incoming-security").setValue("starttls");
    expect((wrapper.get("#incoming-port").element as HTMLInputElement).value).toBe(
      "1993",
    );
  });

  it("submits the exact CreateAccountInput shape", async () => {
    const wrapper = mount(AccountSetupDialog);
    await fillRequiredFields(wrapper);
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(createAccountMock).toHaveBeenCalledWith({
      displayName: "Klaus Ferber",
      email: "klaus@theferbers.eu",
      incoming: {
        protocol: "imap",
        host: "imap.theferbers.eu",
        port: 993,
        security: "tls",
        username: "klaus",
        password: "incoming-secret",
      },
      outgoing: {
        host: "smtp.theferbers.eu",
        port: 465,
        security: "tls",
        username: "klaus",
        password: "outgoing-secret",
      },
    });
    expect(wrapper.emitted("created")?.[0]).toEqual([createdAccount]);
  });

  it("shows the backend error string and stays open on failure", async () => {
    createAccountMock.mockRejectedValue("Authentication failed.");
    const wrapper = mount(AccountSetupDialog);
    await fillRequiredFields(wrapper);
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(wrapper.get('[role="status"]').text()).toContain(
      "Authentication failed.",
    );
    expect(wrapper.emitted("created")).toBeUndefined();
  });

  it("blocks submission and shows guidance when required fields are missing", async () => {
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(createAccountMock).not.toHaveBeenCalled();
    expect(wrapper.get('[role="status"]').text().length).toBeGreaterThan(0);
  });

  it("emits close from the Cancel button and on Escape", async () => {
    const wrapper = mount(AccountSetupDialog);
    await wrapper.get("button.button:not(.primary)").trigger("click");
    expect(wrapper.emitted("close")).toHaveLength(1);

    await wrapper.get(".overlay").trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("close")).toHaveLength(2);
  });
});
