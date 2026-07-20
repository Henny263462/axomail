import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  exportPublicKey,
  generateKey,
  listOwnKeys,
  listRecipientKeys,
} from "../src/api/pgp";
import type { SettingsSectionContext } from "../src/components/settings/context";
import PgpKeysSection from "../src/components/settings/PgpKeysSection.vue";
import type { PgpKeyDto } from "../src/types/pgp";

vi.mock("../src/api/pgp", () => ({
  listOwnKeys: vi.fn(),
  listRecipientKeys: vi.fn(),
  generateKey: vi.fn(),
  importSecretKey: vi.fn(),
  importRecipientKey: vi.fn(),
  exportPublicKey: vi.fn(),
  exportSecretKey: vi.fn(),
  setDefaultKey: vi.fn(),
  deleteOwnKey: vi.fn(),
  deleteRecipientKey: vi.fn(),
}));

const listOwnMock = vi.mocked(listOwnKeys);
const listRecipientsMock = vi.mocked(listRecipientKeys);
const generateMock = vi.mocked(generateKey);
const exportMock = vi.mocked(exportPublicKey);

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

function key(over: Partial<PgpKeyDto>): PgpKeyDto {
  return {
    id: 1,
    fingerprint: "a".repeat(40),
    keyId: "b".repeat(16),
    userId: "Alice <alice@example.org>",
    algorithm: "EdDSALegacy",
    hasPassphrase: false,
    isDefault: true,
    createdAtUnix: 0,
    ...over,
  };
}

describe("PgpKeysSection", () => {
  beforeEach(() => {
    listOwnMock.mockReset();
    listRecipientsMock.mockReset();
    generateMock.mockReset();
    exportMock.mockReset();
    listRecipientsMock.mockResolvedValue([]);
  });

  it("shows the empty state when no keys exist", async () => {
    listOwnMock.mockResolvedValue([]);
    const wrapper = mount(PgpKeysSection, { props: { context } });
    await flushPromises();
    expect(wrapper.text()).toContain("No keys yet");
  });

  it("lists own keys with their fingerprint and default marker", async () => {
    listOwnMock.mockResolvedValue([key({})]);
    const wrapper = mount(PgpKeysSection, { props: { context } });
    await flushPromises();
    expect(wrapper.text()).toContain("Alice <alice@example.org>");
    expect(wrapper.text()).toContain("a".repeat(40));
    expect(wrapper.text()).toContain("Default");
  });

  it("generates a key from the entered user id", async () => {
    listOwnMock.mockResolvedValue([]);
    generateMock.mockResolvedValue(key({}));
    const wrapper = mount(PgpKeysSection, { props: { context } });
    await flushPromises();

    await wrapper
      .find("input[type=text]")
      .setValue("Alice <alice@example.org>");
    // The last button in the "your keys" form-row is Generate.
    const generateButton = wrapper
      .findAll("button")
      .find((b) => b.text() === "Generate key");
    listOwnMock.mockResolvedValue([key({})]);
    await generateButton?.trigger("click");
    await flushPromises();
    expect(generateMock).toHaveBeenCalledWith("Alice <alice@example.org>", null);
  });

  // Finding 7: the exported-key textarea's aria-label is localized, not the
  // former hardcoded lowercase "exported key".
  it("shows an exported key in a localized, labelled textarea", async () => {
    listOwnMock.mockResolvedValue([key({})]);
    exportMock.mockResolvedValue(
      "-----BEGIN PGP PUBLIC KEY BLOCK-----\nabc\n-----END PGP PUBLIC KEY BLOCK-----",
    );
    const wrapper = mount(PgpKeysSection, { props: { context } });
    await flushPromises();

    const exportButton = wrapper
      .findAll("button")
      .find((b) => b.text() === "Export public");
    await exportButton?.trigger("click");
    await flushPromises();

    const area = wrapper.find('textarea[aria-label="Exported key"]');
    expect(area.exists()).toBe(true);
    expect((area.element as HTMLTextAreaElement).value).toContain(
      "BEGIN PGP PUBLIC KEY",
    );
  });

  // Finding 2: the generation passphrase is not retained even when the attempt
  // fails (it never lingers in reactive state after the operation).
  it("clears the passphrase field even when key generation fails", async () => {
    listOwnMock.mockResolvedValue([]);
    generateMock.mockRejectedValue(new Error("boom"));
    const wrapper = mount(PgpKeysSection, { props: { context } });
    await flushPromises();

    await wrapper.find("input[type=text]").setValue("Alice <alice@example.org>");
    await wrapper.find("input[type=password]").setValue("s3cret");
    const generateButton = wrapper
      .findAll("button")
      .find((b) => b.text() === "Generate key");
    await generateButton?.trigger("click");
    await flushPromises();

    expect(generateMock).toHaveBeenCalledWith("Alice <alice@example.org>", "s3cret");
    expect(
      (wrapper.find("input[type=password]").element as HTMLInputElement).value,
    ).toBe("");
  });
});
