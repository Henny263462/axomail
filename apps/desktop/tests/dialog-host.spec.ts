import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import ComposerDialog from "../src/components/compose/ComposerDialog.vue";
import type { AppDialogMap } from "../src/components/layout/dialogs";
import DialogHost from "../src/components/layout/DialogHost.vue";
import SettingsDialog from "../src/components/settings/SettingsDialog.vue";
import AccountSetupDialog from "../src/components/setup/AccountSetupDialog.vue";
import { useDialogs } from "../src/composables/useDialogs";
import type { AccountSummary } from "../src/types/mail";

// The child dialogs reach for the api layer on mount/submit; stub the whole
// boundary so the host can be mounted without a backend.
vi.mock("../src/api/accounts", () => ({
  createAccount: vi.fn(),
  listAccounts: vi.fn(),
}));
vi.mock("../src/api/connection", () => ({ testConnection: vi.fn() }));
vi.mock("../src/api/compose", () => ({ saveDraft: vi.fn(), sendMessage: vi.fn() }));
vi.mock("../src/api/settings", () => ({
  getSettings: vi.fn().mockResolvedValue(null),
  updateSettings: vi.fn(),
  setPop3DeletePolicy: vi.fn(),
}));

const account: AccountSummary = {
  id: 1,
  displayName: "Ferber Mail",
  email: "klaus@theferbers.eu",
  mailboxes: [],
};
const accounts: AccountSummary[] = [account];

function mountHost() {
  const dialogs = useDialogs<AppDialogMap>();
  const wrapper = mount(DialogHost, { props: { dialogs, accounts } });
  return { dialogs, wrapper };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useDialogs", () => {
  it("tracks open state and payloads independently per dialog", () => {
    const dialogs = useDialogs<AppDialogMap>();

    expect(dialogs.isOpen("composer")).toBe(false);
    expect(dialogs.payload("composer")).toBeNull();

    dialogs.open("composer", { accountId: 7, draft: null });
    expect(dialogs.isOpen("composer")).toBe(true);
    expect(dialogs.payload("composer")).toEqual({ accountId: 7, draft: null });
    expect(dialogs.isOpen("settings")).toBe(false);

    dialogs.close("composer");
    expect(dialogs.isOpen("composer")).toBe(false);
    expect(dialogs.payload("composer")).toBeNull();
  });
});

describe("DialogHost", () => {
  it("mounts no dialog until one is opened", () => {
    const { wrapper } = mountHost();
    expect(wrapper.findComponent(AccountSetupDialog).exists()).toBe(false);
    expect(wrapper.findComponent(ComposerDialog).exists()).toBe(false);
    expect(wrapper.findComponent(SettingsDialog).exists()).toBe(false);
  });

  it("mounts and unmounts the setup dialog, keeping its aria wiring", async () => {
    const { dialogs, wrapper } = mountHost();

    dialogs.open("setup");
    await flushPromises();
    const setup = wrapper.findComponent(AccountSetupDialog);
    expect(setup.exists()).toBe(true);
    expect(setup.get('[role="dialog"]').attributes("aria-labelledby")).toBe(
      "account-setup-title",
    );

    dialogs.close("setup");
    await flushPromises();
    expect(wrapper.findComponent(AccountSetupDialog).exists()).toBe(false);
  });

  it("forwards the composer payload as props", async () => {
    const { dialogs, wrapper } = mountHost();

    dialogs.open("composer", { accountId: 42, draft: null });
    await flushPromises();
    const composer = wrapper.findComponent(ComposerDialog);
    expect(composer.exists()).toBe(true);
    expect(composer.props("accountId")).toBe(42);
  });

  it("forwards accounts to the settings dialog", async () => {
    const { dialogs, wrapper } = mountHost();

    dialogs.open("settings");
    await flushPromises();
    const settings = wrapper.findComponent(SettingsDialog);
    expect(settings.exists()).toBe(true);
    expect(settings.props("accounts")).toEqual(accounts);
  });

  it("re-emits each dialog's domain events to the parent", async () => {
    const { dialogs, wrapper } = mountHost();

    dialogs.open("setup");
    await flushPromises();
    wrapper.findComponent(AccountSetupDialog).vm.$emit("created", account);
    expect(wrapper.emitted("accountCreated")?.[0]).toEqual([account]);

    dialogs.open("composer", { accountId: 1, draft: null });
    await flushPromises();
    const composer = wrapper.findComponent(ComposerDialog);
    composer.vm.$emit("sent", { draftId: 3 });
    expect(wrapper.emitted("composerSent")?.[0]).toEqual([{ draftId: 3 }]);
    composer.vm.$emit("saved", 3);
    expect(wrapper.emitted("composerSaved")).toHaveLength(1);
  });

  it("closes a dialog when it emits close", async () => {
    const { dialogs, wrapper } = mountHost();

    dialogs.open("settings");
    await flushPromises();
    wrapper.findComponent(SettingsDialog).vm.$emit("close");
    await flushPromises();
    expect(wrapper.findComponent(SettingsDialog).exists()).toBe(false);
  });
});
