import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createAccount, deleteAccount } from "../src/api/accounts";
import { oauthAuthorize } from "../src/api/oauth";
import AccountSetupDialog from "../src/components/setup/AccountSetupDialog.vue";
import SidebarPane from "../src/components/sidebar/SidebarPane.vue";
import type { AccountSummary } from "../src/types/mail";

vi.mock("../src/api/accounts", () => ({
  listAccounts: vi.fn(),
  createAccount: vi.fn(),
  deleteAccount: vi.fn(),
}));

vi.mock("../src/api/oauth", () => ({
  oauthAuthorize: vi.fn(),
  testAccountConnection: vi.fn(),
}));

const createAccountMock = vi.mocked(createAccount);
const deleteAccountMock = vi.mocked(deleteAccount);
const oauthAuthorizeMock = vi.mocked(oauthAuthorize);

/** Fills the required fields for an OAuth2 account with the given client id. */
async function fillOAuthFields(
  wrapper: VueWrapper,
  clientId: string,
): Promise<void> {
  await wrapper.get("#account-display-name").setValue("Klaus Ferber");
  await wrapper.get("#account-email").setValue("klaus@gmail.com");
  await wrapper.get("#auth-mechanism").setValue("oauth2");
  await wrapper.get("#oauth-client-id").setValue(clientId);
  await wrapper.get("#incoming-host").setValue("imap.gmail.com");
  await wrapper.get("#incoming-username").setValue("klaus@gmail.com");
  await wrapper.get("#outgoing-host").setValue("smtp.gmail.com");
  await wrapper.get("#outgoing-username").setValue("klaus@gmail.com");
}

const createdAccount: AccountSummary = {
  id: 7,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
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
  deleteAccountMock.mockResolvedValue(null);
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
      auth: { mechanism: "password" },
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

  it("runs the OAuth2 flow: creates the account, then signs in", async () => {
    oauthAuthorizeMock.mockResolvedValue({ status: "authorized" });
    const wrapper = mount(AccountSetupDialog);

    await wrapper.get("#account-display-name").setValue("Klaus Ferber");
    await wrapper.get("#account-email").setValue("klaus@gmail.com");
    await wrapper.get("#auth-mechanism").setValue("oauth2");
    await wrapper.get("#oauth-client-id").setValue("cid.apps.googleusercontent.com");
    await wrapper.get("#incoming-host").setValue("imap.gmail.com");
    await wrapper.get("#incoming-username").setValue("klaus@gmail.com");
    await wrapper.get("#outgoing-host").setValue("smtp.gmail.com");
    await wrapper.get("#outgoing-username").setValue("klaus@gmail.com");
    // No password inputs are rendered in OAuth2 mode.
    expect(wrapper.find("#incoming-password").exists()).toBe(false);

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    const payload = createAccountMock.mock.calls[0]?.[0];
    expect(payload?.auth).toEqual({
      mechanism: "oauth2",
      oauth: { provider: "gmail", clientId: "cid.apps.googleusercontent.com" },
    });
    expect(payload?.incoming.password).toBe("");
    expect(oauthAuthorizeMock).toHaveBeenCalledWith(7);
    expect(wrapper.emitted("created")?.[0]).toEqual([createdAccount]);
  });

  it("keeps the dialog open and explains a declined OAuth2 sign-in", async () => {
    oauthAuthorizeMock.mockResolvedValue({ status: "denied" });
    const wrapper = mount(AccountSetupDialog);

    await wrapper.get("#account-display-name").setValue("Klaus Ferber");
    await wrapper.get("#account-email").setValue("klaus@gmail.com");
    await wrapper.get("#auth-mechanism").setValue("oauth2");
    await wrapper.get("#oauth-client-id").setValue("cid");
    await wrapper.get("#incoming-host").setValue("imap.gmail.com");
    await wrapper.get("#incoming-username").setValue("klaus@gmail.com");
    await wrapper.get("#outgoing-host").setValue("smtp.gmail.com");
    await wrapper.get("#outgoing-username").setValue("klaus@gmail.com");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(wrapper.get('[role="status"]').text()).toContain("declined");
    expect(wrapper.emitted("created")).toBeUndefined();
    // A second submit reuses the same created account rather than recreating.
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(createAccountMock).toHaveBeenCalledTimes(1);
    expect(oauthAuthorizeMock).toHaveBeenCalledTimes(2);
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

describe("AccountSetupDialog OAuth2 retry cleanup", () => {
  it("re-creates with the edited config on retry, deleting the stale row", async () => {
    oauthAuthorizeMock.mockResolvedValue({ status: "denied" });
    const wrapper = mount(AccountSetupDialog);
    await fillOAuthFields(wrapper, "cid-original");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    // Account created, sign-in declined; the row is kept for a retry.
    expect(createAccountMock).toHaveBeenCalledTimes(1);
    expect(deleteAccountMock).not.toHaveBeenCalled();

    // Editing any field invalidates the stale unauthorized row.
    await wrapper.get("#oauth-client-id").setValue("cid-edited");
    await flushPromises();
    expect(deleteAccountMock).toHaveBeenCalledWith(7);

    // The retry creates a fresh account carrying the edited client id.
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(createAccountMock).toHaveBeenCalledTimes(2);
    expect(createAccountMock.mock.calls[1]?.[0].auth).toEqual({
      mechanism: "oauth2",
      oauth: { provider: "gmail", clientId: "cid-edited" },
    });
    wrapper.unmount();
  });

  it("deletes the orphan account when the dialog closes without success", async () => {
    oauthAuthorizeMock.mockResolvedValue({ status: "denied" });
    const wrapper = mount(AccountSetupDialog);
    await fillOAuthFields(wrapper, "cid");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(deleteAccountMock).not.toHaveBeenCalled();

    wrapper.unmount();
    await flushPromises();
    expect(deleteAccountMock).toHaveBeenCalledWith(7);
  });

  it("never deletes a successfully authorized account on close", async () => {
    oauthAuthorizeMock.mockResolvedValue({ status: "authorized" });
    const wrapper = mount(AccountSetupDialog);
    await fillOAuthFields(wrapper, "cid");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(wrapper.emitted("created")).toBeTruthy();

    // Closing after success must not touch the created account.
    wrapper.unmount();
    await flushPromises();
    expect(deleteAccountMock).not.toHaveBeenCalled();
  });
});
