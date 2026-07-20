import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getLanguage, setLanguage } from "../src/api/settings";
import type { SettingsSectionContext } from "../src/components/settings/context";
import LanguageSection from "../src/components/settings/LanguageSection.vue";
import {
  i18n,
  initLanguage,
  reconcileLanguage,
  resolveLocale,
  setLanguagePreference,
} from "../src/i18n";
import { isTauri } from "../src/lib/tauri";

vi.mock("../src/api/settings", () => ({
  getLanguage: vi.fn(),
  setLanguage: vi.fn(),
}));
vi.mock("../src/lib/tauri", () => ({ isTauri: vi.fn() }));

const getLanguageMock = vi.mocked(getLanguage);
const setLanguageMock = vi.mocked(setLanguage);
const isTauriMock = vi.mocked(isTauri);

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  // Reset the module's pin + locale to a known, un-pinned baseline.
  initLanguage();
  getLanguageMock.mockResolvedValue("system");
  setLanguageMock.mockResolvedValue(null);
  isTauriMock.mockReturnValue(true);
});

describe("i18n language preference module", () => {
  it("resolves concrete preferences and falls back to English for system", () => {
    expect(resolveLocale("en")).toBe("en");
    expect(resolveLocale("de")).toBe("de");
    // `system` resolves to a shipped locale (en unless the OS is German).
    expect(["en", "de"]).toContain(resolveLocale("system"));
  });

  it("paints the cached preference on init", () => {
    localStorage.setItem("axomail.language", "de");
    initLanguage();
    expect(i18n.global.locale.value).toBe("de");
  });

  it("pins a user choice so a later backend reconcile is a no-op", () => {
    setLanguagePreference("de");
    expect(i18n.global.locale.value).toBe("de");
    // A slow startup read resolving the old value must not stomp the choice.
    reconcileLanguage("en");
    expect(i18n.global.locale.value).toBe("de");
  });

  it("applies the backend value on reconcile when the user has not chosen", () => {
    initLanguage(); // un-pinned baseline
    reconcileLanguage("de");
    expect(i18n.global.locale.value).toBe("de");
  });
});

/** A minimal settings context; the section only uses the two report hooks. */
function stubContext(
  over: Partial<SettingsSectionContext> = {},
): SettingsSectionContext {
  return {
    accounts: [],
    initialPolicies: [],
    notificationsEnabled: true,
    syncIntervalMinutes: 10,
    theme: "system",
    autoMarkReadEnabled: true,
    autoMarkReadDelayMs: 0,
    setNotificationsEnabled: vi.fn(),
    setSyncIntervalMinutes: vi.fn(),
    setAutoMarkRead: vi.fn(),
    setTheme: vi.fn(),
    reportError: vi.fn(),
    reportStatus: vi.fn(),
    notifyAccountsChanged: vi.fn(),
    notifyTagsChanged: vi.fn(),
    ...over,
  };
}

describe("LanguageSection", () => {
  it("loads the persisted preference and checks its radio", async () => {
    getLanguageMock.mockResolvedValue("de");
    const wrapper = mount(LanguageSection, { props: { context: stubContext() } });
    await flushPromises();

    expect(getLanguageMock).toHaveBeenCalledOnce();
    const de = wrapper.get<HTMLInputElement>('input[value="de"]');
    expect(de.element.checked).toBe(true);
    // The chrome renders through t() in the default (English) locale.
    expect(wrapper.text()).toContain("Language");
  });

  it("persists and applies a new language, reporting success", async () => {
    getLanguageMock.mockResolvedValue("en");
    const context = stubContext();
    const wrapper = mount(LanguageSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[value="de"]').trigger("change");
    await flushPromises();

    expect(setLanguageMock).toHaveBeenCalledWith("de");
    expect(i18n.global.locale.value).toBe("de");
    // The confirmation localizes to the newly-chosen language (the UI has just
    // switched to German), proving the message goes through the live catalogue.
    expect(context.reportStatus).toHaveBeenLastCalledWith("Sprache aktualisiert.");
  });

  it("reverts the locale and reports the error when persistence fails", async () => {
    getLanguageMock.mockResolvedValue("en");
    setLanguageMock.mockRejectedValueOnce(new Error("disk full"));
    const context = stubContext();
    const wrapper = mount(LanguageSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[value="de"]').trigger("change");
    await flushPromises();

    // The optimistic switch is rolled back and the failure surfaced.
    expect(i18n.global.locale.value).toBe("en");
    expect(context.reportError).toHaveBeenCalledWith("disk full");
    const en = wrapper.get<HTMLInputElement>('input[value="en"]');
    expect(en.element.checked).toBe(true);
  });

  it("does not persist in the browser preview and explains why", async () => {
    isTauriMock.mockReturnValue(false);
    const context = stubContext();
    const wrapper = mount(LanguageSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[value="de"]').trigger("change");
    await flushPromises();

    expect(setLanguageMock).not.toHaveBeenCalled();
    expect(context.reportError).toHaveBeenCalledOnce();
  });
});
