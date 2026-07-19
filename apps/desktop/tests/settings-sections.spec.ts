import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { getSettings, setPop3DeletePolicy } from "../src/api/settings";
import AccountsSection from "../src/components/settings/AccountsSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import NotificationsSection from "../src/components/settings/NotificationsSection.vue";
import Pop3DeleteSection from "../src/components/settings/Pop3DeleteSection.vue";
import { settingsSections } from "../src/components/settings/sections";
import SettingsDialog from "../src/components/settings/SettingsDialog.vue";
import SyncIntervalSection from "../src/components/settings/SyncIntervalSection.vue";
import type { AccountSummary, SettingsDto } from "../src/types/mail";

vi.mock("../src/api/settings", () => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  setPop3DeletePolicy: vi.fn(),
}));

// The Tags section loads its list on mount; stub the tag api so the registry
// tests can mount it without a live backend.
vi.mock("../src/api/tags", () => ({
  listTags: vi.fn().mockResolvedValue([]),
  createTag: vi.fn(),
  updateTag: vi.fn(),
  deleteTag: vi.fn(),
}));

const getSettingsMock = vi.mocked(getSettings);
const setPop3DeletePolicyMock = vi.mocked(setPop3DeletePolicy);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const accounts: AccountSummary[] = [
  {
    id: 2,
    displayName: "Legacy POP",
    email: "work@example.org",
    authMechanism: "password",
    mailboxes: [],
  },
];

/** A minimal context so a section can be exercised in isolation. */
function stubContext(
  overrides: Partial<SettingsSectionContext> = {},
): SettingsSectionContext {
  return {
    accounts,
    initialPolicies: [{ accountId: 2, enabled: false }],
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
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  // Simulate the desktop shell so autonomous sections actually invoke.
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  getSettingsMock.mockResolvedValue(null);
  setPop3DeletePolicyMock.mockResolvedValue(null);
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("settings section registry", () => {
  it("registers each section with an id, title, and component", () => {
    expect(settingsSections.length).toBeGreaterThan(0);
    for (const section of settingsSections) {
      expect(typeof section.id).toBe("string");
      expect(section.id.length).toBeGreaterThan(0);
      expect(typeof section.title).toBe("string");
      expect(section.component).toBeTruthy();
    }
    expect(settingsSections.map((section) => section.id)).toEqual([
      "notifications",
      "sync-interval",
      "auto-read",
      "appearance",
      "pop3-delete",
      "identities",
      "tags",
      "accounts",
    ]);
  });

  it("mounts every registered section from just its context prop", () => {
    // The registration contract: a section is a self-contained,
    // context-driven component, so the dialog renders any entry uniformly.
    for (const section of settingsSections) {
      const wrapper = mount(section.component, {
        props: { context: stubContext() },
      });
      expect(wrapper.html().length).toBeGreaterThan(0);
      wrapper.unmount();
    }
  });
});

describe("NotificationsSection", () => {
  it("reflects and stages the notifications flag with a visible label", async () => {
    const context = stubContext({ notificationsEnabled: true });
    const wrapper = mount(NotificationsSection, { props: { context } });

    expect(
      (wrapper.get("#settings-notifications").element as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(wrapper.text()).toContain("Show desktop notifications for new mail");

    await wrapper.get("#settings-notifications").setValue(false);
    expect(context.setNotificationsEnabled).toHaveBeenCalledWith(false);
  });
});

describe("SyncIntervalSection", () => {
  it("reflects and stages the sync interval with a labelled field", async () => {
    const context = stubContext({ syncIntervalMinutes: 5 });
    const wrapper = mount(SyncIntervalSection, { props: { context } });

    expect(
      (wrapper.get("#settings-interval").element as HTMLInputElement).value,
    ).toBe("5");
    expect(wrapper.find('label[for="settings-interval"]').exists()).toBe(true);

    await wrapper.get("#settings-interval").setValue("15");
    expect(context.setSyncIntervalMinutes).toHaveBeenCalledWith(15);
  });
});

describe("Pop3DeleteSection", () => {
  it("shows the guarded fieldset and flips a policy via the command", async () => {
    const context = stubContext();
    const wrapper = mount(Pop3DeleteSection, { props: { context } });

    expect(wrapper.get("legend").text()).toContain("POP3 server deletion");
    expect(wrapper.text()).toContain("This cannot be undone.");
    expect(wrapper.text()).toContain("Legacy POP");

    await wrapper.get('input[type="checkbox"]').setValue(true);
    await flushPromises();

    expect(setPop3DeletePolicyMock).toHaveBeenCalledWith(2, true);
    expect(context.reportError).toHaveBeenLastCalledWith("");
  });

  it("refuses the toggle and reports an error in the browser preview", async () => {
    delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
    const context = stubContext();
    const wrapper = mount(Pop3DeleteSection, { props: { context } });

    await wrapper.get('input[type="checkbox"]').setValue(true);

    expect(setPop3DeletePolicyMock).not.toHaveBeenCalled();
    expect(context.reportError).toHaveBeenCalledWith(
      expect.stringContaining("browser preview"),
    );
  });

  it("renders nothing when no POP3 account carries a policy", () => {
    const wrapper = mount(Pop3DeleteSection, {
      props: { context: stubContext({ initialPolicies: [] }) },
    });
    expect(wrapper.find("fieldset").exists()).toBe(false);
  });
});

describe("SettingsDialog registry integration", () => {
  const settings: SettingsDto = {
    notificationsEnabled: false,
    syncIntervalMinutes: 5,
    theme: "system",
    autoMarkReadEnabled: true,
    autoMarkReadDelayMs: 0,
    pop3DeletePolicies: [{ accountId: 2, enabled: false }],
  };

  it("renders every registered section inside the modal shell", async () => {
    getSettingsMock.mockResolvedValue({
      ...settings,
      pop3DeletePolicies: settings.pop3DeletePolicies.map((p) => ({ ...p })),
    });
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    // One marker per registered section proves the shell is registry-driven.
    expect(wrapper.find("#settings-notifications").exists()).toBe(true);
    expect(wrapper.find("#settings-interval").exists()).toBe(true);
    expect(wrapper.find('input[name="settings-theme"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("POP3 server deletion");

    // The shell keeps the overlay/aria-modal plumbing around the sections.
    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.attributes("aria-modal")).toBe("true");
    expect(dialog.attributes("aria-labelledby")).toBe("settings-title");
  });

  it("forwards a section's notifyAccountsChanged as an accountsChanged event", async () => {
    getSettingsMock.mockResolvedValue({
      ...settings,
      pop3DeletePolicies: settings.pop3DeletePolicies.map((p) => ({ ...p })),
    });
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    const section = wrapper.findComponent(AccountsSection);
    expect(section.exists()).toBe(true);
    // The callback the section receives via context must reach the parent so
    // the app shell (sidebar, selection) can reconcile the change.
    (section.props("context") as SettingsSectionContext).notifyAccountsChanged();

    expect(wrapper.emitted("accountsChanged")).toHaveLength(1);
  });
});
