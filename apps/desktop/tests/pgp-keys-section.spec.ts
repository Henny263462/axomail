import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
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
});
