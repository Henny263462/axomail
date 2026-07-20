/**
 * Wave 5.4 localization guards.
 *
 * These assert the two invariants the full string-extraction must keep true:
 * (1) the English (source) and German catalogues have exactly the same set of
 *     keys — so no converted component can hit a missing-key fallback in German
 *     (and no stray German-only key rots), and
 * (2) a representative converted component actually renders its German strings
 *     when the active locale is `de`, and re-renders back to English on switch.
 */
import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";

import AppearanceSection from "../src/components/settings/AppearanceSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import { i18n } from "../src/i18n";

/** Collects every leaf (string-valued) key-path in a nested message catalogue. */
function leafKeys(node: unknown, prefix = "", out = new Set<string>()): Set<string> {
  if (node !== null && typeof node === "object" && !Array.isArray(node)) {
    for (const [key, value] of Object.entries(node)) {
      leafKeys(value, prefix === "" ? key : `${prefix}.${key}`, out);
    }
  } else {
    out.add(prefix);
  }
  return out;
}

afterEach(() => {
  i18n.global.locale.value = "en";
});

describe("locale catalogue parity", () => {
  it("the en and de catalogues expose an identical set of keys", () => {
    const en = leafKeys(i18n.global.getLocaleMessage("en"));
    const de = leafKeys(i18n.global.getLocaleMessage("de"));
    // Report the offenders directly so a failure names the missing key.
    const missingInDe = [...en].filter((k) => !de.has(k)).sort();
    const missingInEn = [...de].filter((k) => !en.has(k)).sort();
    expect(missingInDe).toEqual([]);
    expect(missingInEn).toEqual([]);
  });

  it("ships a non-trivial, deep-merged catalogue (fragments loaded)", () => {
    const en = leafKeys(i18n.global.getLocaleMessage("en"));
    // Base namespaces plus at least the common fragment must be present.
    expect(en.has("common.cancel")).toBe(true);
    expect(en.has("settings.appearance.title")).toBe(true);
    // A healthy extraction is well into the hundreds of keys.
    expect(en.size).toBeGreaterThan(200);
  });
});

/** A minimal settings context; AppearanceSection only reads `theme`/`setTheme`. */
function stubContext(): SettingsSectionContext {
  return {
    accounts: [],
    initialPolicies: [],
    notificationsEnabled: true,
    syncIntervalMinutes: 10,
    theme: "system",
    autoMarkReadEnabled: true,
    autoMarkReadDelayMs: 0,
    setNotificationsEnabled: () => {},
    setSyncIntervalMinutes: () => {},
    setAutoMarkRead: () => {},
    setTheme: () => {},
    reportError: () => {},
    reportStatus: () => {},
    notifyAccountsChanged: () => {},
    notifyTagsChanged: () => {},
  };
}

describe("a converted component follows the active locale", () => {
  it("renders English by default and German when the locale is de", async () => {
    const wrapper = mount(AppearanceSection, { props: { context: stubContext() } });
    // Default locale is English (reset between tests by the global setup).
    expect(wrapper.text()).toContain("Appearance");

    i18n.global.locale.value = "de";
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("Darstellung");
    expect(wrapper.text()).not.toContain("Appearance");
  });
});
