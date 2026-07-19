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
  {
    id: 1,
    displayName: "Ferber Mail",
    email: "klaus@theferbers.eu",
    authMechanism: "password",
    mailboxes: [],
  },
  {
    id: 2,
    displayName: "Legacy POP",
    email: "work@example.org",
    authMechanism: "password",
    mailboxes: [],
  },
];

const settings: SettingsDto = {
  notificationsEnabled: false,
  syncIntervalMinutes: 5,
  theme: "system",
  autoMarkReadEnabled: true,
  autoMarkReadDelayMs: 0,
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
  // Keep the module-level theme cache and the document root from leaking
  // between tests (the dialog applies the theme live).
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
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
      theme: "system",
      autoMarkReadEnabled: true,
      autoMarkReadDelayMs: 0,
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

  it("shows the Appearance section with the stored theme selected", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    expect(wrapper.text()).toContain("Appearance");
    const radios = wrapper.findAll<HTMLInputElement>(
      'input[name="settings-theme"]',
    );
    expect(radios.map((radio) => radio.element.value)).toEqual([
      "system",
      "light",
      "dark",
    ]);
    const checked = radios.find((radio) => radio.element.checked);
    expect(checked?.element.value).toBe("system");
  });

  it("previews a theme selection live and saves it with the settings", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    const dark = wrapper
      .findAll<HTMLInputElement>('input[name="settings-theme"]')
      .find((radio) => radio.element.value === "dark");
    await dark?.setValue();

    // The selection reaches the document root immediately (live preview).
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ theme: "dark" }),
    );
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("reverts a previewed theme when cancelled without saving", async () => {
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    const dark = wrapper
      .findAll<HTMLInputElement>('input[name="settings-theme"]')
      .find((radio) => radio.element.value === "dark");
    await dark?.setValue();
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

    const cancel = wrapper
      .findAll("button")
      .find((button) => button.text() === "Cancel");
    await cancel?.trigger("click");
    await flushPromises();

    // The stored theme was "system", which resolves to light in this
    // environment; the preview is undone and nothing is persisted.
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(updateSettingsMock).not.toHaveBeenCalled();
    expect(wrapper.emitted("close")).toHaveLength(1);
  });

  it("surfaces a save failure via the alert region", async () => {
    updateSettingsMock.mockRejectedValue("Interval too large.");
    const wrapper = mount(SettingsDialog, { props: { accounts } });
    await flushPromises();

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    // The dialog's own shared alert region (a direct child of the settings
    // form) carries the save failure; registered sections have their own
    // alert regions, so target the dialog's specifically.
    expect(wrapper.get('.form > [role="alert"]').text()).toContain(
      "Interval too large.",
    );
    expect(wrapper.emitted("close")).toBeUndefined();
  });
});
