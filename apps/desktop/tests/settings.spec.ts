import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getSettings,
  setPop3DeletePolicy,
  updateSettings,
} from "../src/api/settings";
import SettingsDialog from "../src/components/settings/SettingsDialog.vue";
import type { AccountSummary, SettingsDto } from "../src/types/mail";

vi.mock("../src/api/settings", () => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  setPop3DeletePolicy: vi.fn(),
}));

const getSettingsMock = vi.mocked(getSettings);
const updateSettingsMock = vi.mocked(updateSettings);
const setPop3DeletePolicyMock = vi.mocked(setPop3DeletePolicy);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const accounts: AccountSummary[] = [
  { id: 1, displayName: "Ferber Mail", email: "klaus@theferbers.eu", mailboxes: [] },
  { id: 2, displayName: "Legacy POP", email: "work@example.org", mailboxes: [] },
];

const settings: SettingsDto = {
  notificationsEnabled: false,
  syncIntervalMinutes: 5,
  pop3DeletePolicies: [{ accountId: 2, enabled: false }],
};

beforeEach(() => {
  vi.clearAllMocks();
  // Simulate the desktop shell so the dialog's mutations actually invoke.
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  getSettingsMock.mockResolvedValue({
    ...settings,
    pop3DeletePolicies: settings.pop3DeletePolicies.map((p) => ({ ...p })),
  });
  updateSettingsMock.mockResolvedValue(null);
  setPop3DeletePolicyMock.mockResolvedValue(null);
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("SettingsDialog", () => {
  it("loads and displays the current settings with the POP3 warning", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    expect(getSettingsMock).toHaveBeenCalledTimes(1);
    expect(
      (wrapper.get("#settings-notifications").element as HTMLInputElement).checked,
    ).toBe(false);
    expect(
      (wrapper.get("#settings-interval").element as HTMLInputElement).value,
    ).toBe("5");
    expect(wrapper.text()).toContain("Legacy POP");
    expect(wrapper.text()).toContain(
      "Deletes mail from the server after download. This cannot be undone.",
    );
  });

  it("saves notification and interval changes via updateSettings", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    await wrapper.get("#settings-notifications").setValue(true);
    await wrapper.get("#settings-interval").setValue("15");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(updateSettingsMock).toHaveBeenCalledWith({
      notificationsEnabled: true,
      syncIntervalMinutes: 15,
      pop3DeletePolicies: [{ accountId: 2, enabled: false }],
    });
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("flips a POP3 delete policy through the dedicated command", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    await wrapper.get('fieldset input[type="checkbox"]').setValue(true);
    await flushPromises();

    expect(setPop3DeletePolicyMock).toHaveBeenCalledWith(2, true);
    // updateSettings is NOT what enables deletion — the dedicated call is.
    expect(updateSettingsMock).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("This cannot be undone.");
  });

  it("surfaces a save failure via the alert region", async () => {
    updateSettingsMock.mockRejectedValue("Interval too large.");
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "Interval too large.",
    );
    expect(wrapper.emitted("close")).toBeUndefined();
  });
});
