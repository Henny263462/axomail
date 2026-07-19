import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createIdentity,
  deleteIdentity,
  listIdentities,
  setDefaultIdentity,
} from "../src/api/identities";
import IdentitiesSection from "../src/components/settings/IdentitiesSection.vue";
import type { AccountSummary, IdentityDto } from "../src/types/mail";

vi.mock("../src/api/identities", () => ({
  listIdentities: vi.fn(),
  createIdentity: vi.fn(),
  updateIdentity: vi.fn(),
  deleteIdentity: vi.fn(),
  setDefaultIdentity: vi.fn(),
}));

const listIdentitiesMock = vi.mocked(listIdentities);
const createIdentityMock = vi.mocked(createIdentity);
const deleteIdentityMock = vi.mocked(deleteIdentity);
const setDefaultIdentityMock = vi.mocked(setDefaultIdentity);

const accounts: AccountSummary[] = [
  { id: 1, displayName: "Work", email: "klaus@theferbers.eu", mailboxes: [] },
];

function identity(over: Partial<IdentityDto>): IdentityDto {
  return {
    id: 1,
    accountId: 1,
    email: "klaus@theferbers.eu",
    displayName: "",
    replyTo: null,
    signatureText: null,
    signaturePlacement: "below_quote",
    replyPosition: "below",
    forwardMode: "inline",
    archiveGranularity: "flat",
    isDefault: false,
    sortPosition: 0,
    sentDest: null,
    draftsDest: null,
    templatesDest: null,
    archiveDest: null,
    ...over,
  };
}

function buttonByText(wrapper: VueWrapper, label: string) {
  const button = wrapper.findAll("button").find((b) => b.text() === label);
  if (button === undefined) throw new Error(`button "${label}" missing`);
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  deleteIdentityMock.mockResolvedValue(null);
  setDefaultIdentityMock.mockResolvedValue(null);
});

describe("IdentitiesSection", () => {
  it("lists an account's identities with a default badge", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 1, isDefault: true }),
      identity({ id: 2, email: "sales@theferbers.eu" }),
    ]);
    const wrapper = mount(IdentitiesSection, { props: { accounts } });
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain("klaus@theferbers.eu");
    expect(text).toContain("sales@theferbers.eu");
    expect(text).toContain("Default");
  });

  it("creates an identity through the inline form", async () => {
    listIdentitiesMock.mockResolvedValueOnce([identity({ id: 1, isDefault: true })]);
    listIdentitiesMock.mockResolvedValueOnce([
      identity({ id: 1, isDefault: true }),
      identity({ id: 2, email: "sales@theferbers.eu" }),
    ]);
    createIdentityMock.mockResolvedValue(identity({ id: 2, email: "sales@theferbers.eu" }));

    const wrapper = mount(IdentitiesSection, { props: { accounts } });
    await flushPromises();

    await buttonByText(wrapper, "Add identity").trigger("click");
    await wrapper.get("#identity-email").setValue("sales@theferbers.eu");
    await wrapper.get("form.identity-form").trigger("submit");
    await flushPromises();

    expect(createIdentityMock).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 1, email: "sales@theferbers.eu" }),
    );
    expect(listIdentitiesMock).toHaveBeenCalledTimes(2);
  });

  it("removes a non-default identity when more than one exists", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 1, isDefault: true }),
      identity({ id: 2, email: "sales@theferbers.eu" }),
    ]);
    const wrapper = mount(IdentitiesSection, { props: { accounts } });
    await flushPromises();

    await buttonByText(wrapper, "Remove").trigger("click");
    await flushPromises();

    expect(deleteIdentityMock).toHaveBeenCalledWith(2);
  });

  it("promotes an identity to default", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 1, isDefault: true }),
      identity({ id: 2, email: "sales@theferbers.eu" }),
    ]);
    const wrapper = mount(IdentitiesSection, { props: { accounts } });
    await flushPromises();

    await buttonByText(wrapper, "Make default").trigger("click");
    await flushPromises();

    expect(setDefaultIdentityMock).toHaveBeenCalledWith(2);
  });

  it("reports the browser preview as unavailable", async () => {
    listIdentitiesMock.mockResolvedValue(null);
    const wrapper = mount(IdentitiesSection, { props: { accounts } });
    await flushPromises();

    expect(wrapper.text()).toContain("unavailable in the browser preview");
  });
});
