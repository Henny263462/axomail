import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  createFilter,
  deleteFilter,
  getAccountFilterSettings,
  listFilters,
  reorderFilters,
  runFiltersOnFolder,
  setAccountFilterArrival,
  setFilterEnabled,
} from "../src/api/filters";
import type { SettingsSectionContext } from "../src/components/settings/context";
import FiltersSection from "../src/components/settings/FiltersSection.vue";
import type { AccountSummary, FilterDto } from "../src/types/mail";

vi.mock("../src/api/filters", () => ({
  listFilters: vi.fn(),
  createFilter: vi.fn(),
  updateFilter: vi.fn(),
  deleteFilter: vi.fn(),
  setFilterEnabled: vi.fn(),
  reorderFilters: vi.fn(),
  runFiltersOnFolder: vi.fn(),
  getAccountFilterSettings: vi.fn(),
  setAccountFilterArrival: vi.fn(),
}));

const listFiltersMock = vi.mocked(listFilters);
const createFilterMock = vi.mocked(createFilter);
const deleteFilterMock = vi.mocked(deleteFilter);
const setFilterEnabledMock = vi.mocked(setFilterEnabled);
const reorderFiltersMock = vi.mocked(reorderFilters);
const runFiltersOnFolderMock = vi.mocked(runFiltersOnFolder);
const getSettingsMock = vi.mocked(getAccountFilterSettings);
const setArrivalMock = vi.mocked(setAccountFilterArrival);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const account: AccountSummary = {
  id: 1,
  displayName: "Work",
  email: "klaus@theferbers.eu",
  authMechanism: "password",
  mailboxes: [
    { id: 10, name: "Inbox", role: "inbox", unreadCount: 0 },
    { id: 11, name: "Archive", role: "archive", unreadCount: 0 },
  ],
};

const context = {
  accounts: [account],
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

function filter(id: number, name: string): FilterDto {
  return {
    id,
    accountId: 1,
    name,
    enabled: true,
    matchType: "all",
    sortOrder: id,
    stopOnMatch: false,
    conditions: [{ field: "subject", op: "contains", value: "x" }],
    actions: [{ kind: "star", param: "" }],
  };
}

async function mountWith(filters: FilterDto[]): Promise<VueWrapper> {
  listFiltersMock.mockResolvedValue(filters);
  const wrapper = mount(FiltersSection, {
    props: { context },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  vi.clearAllMocks();
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  createFilterMock.mockResolvedValue(null);
  deleteFilterMock.mockResolvedValue(null);
  setFilterEnabledMock.mockResolvedValue(null);
  reorderFiltersMock.mockResolvedValue(null);
  runFiltersOnFolderMock.mockResolvedValue({
    matched: 2,
    actionsApplied: 2,
    forwardsQueued: 0,
  });
  getSettingsMock.mockResolvedValue({ arrivalEnabled: true });
  setArrivalMock.mockResolvedValue(null);
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("FiltersSection", () => {
  it("shows an honest empty state when there are no filters", async () => {
    const wrapper = await mountWith([]);
    expect(listFiltersMock).toHaveBeenCalledWith(1);
    expect(wrapper.text()).toContain("No filters yet");
    expect(wrapper.find(".filter-list").exists()).toBe(false);
    wrapper.unmount();
  });

  it("lists filters with an enable toggle", async () => {
    const wrapper = await mountWith([filter(1, "Invoices"), filter(2, "Newsletters")]);
    expect(wrapper.text()).toContain("Invoices");
    expect(wrapper.text()).toContain("Newsletters");
    wrapper.unmount();
  });

  it("creates a filter through the editor and reloads", async () => {
    const wrapper = await mountWith([]);
    await wrapper.findAll("button").find((b) => b.text() === "Add filter")!.trigger("click");
    await flushPromises();

    await wrapper.get('input[aria-label="Filter name"]').setValue("File invoices");
    await wrapper.findAll("button").find((b) => b.text() === "Save filter")!.trigger("click");
    await flushPromises();

    expect(createFilterMock).toHaveBeenCalledTimes(1);
    expect(createFilterMock.mock.calls[0]![0].name).toBe("File invoices");
    // Mount load + reload after the save.
    expect(listFiltersMock).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });

  it("reorders filters with the move controls", async () => {
    const wrapper = await mountWith([filter(1, "Alpha"), filter(2, "Beta")]);
    await wrapper.get('button[aria-label="Move Alpha down"]').trigger("click");
    await flushPromises();
    expect(reorderFiltersMock).toHaveBeenCalledWith(1, [2, 1]);
    wrapper.unmount();
  });

  it("toggles a filter's enabled flag", async () => {
    const wrapper = await mountWith([filter(1, "Alpha")]);
    await wrapper.get('input[aria-label="Enable Alpha"]').trigger("change");
    await flushPromises();
    expect(setFilterEnabledMock).toHaveBeenCalledWith(1, false);
    wrapper.unmount();
  });

  it("deletes a filter", async () => {
    const wrapper = await mountWith([filter(1, "Alpha")]);
    await wrapper.findAll("button").find((b) => b.text() === "Delete")!.trigger("click");
    await flushPromises();
    expect(deleteFilterMock).toHaveBeenCalledWith(1);
    wrapper.unmount();
  });

  it("runs filters over the chosen folder", async () => {
    const wrapper = await mountWith([filter(1, "Alpha")]);
    await wrapper.findAll("button").find((b) => b.text() === "Run now")!.trigger("click");
    await flushPromises();
    expect(runFiltersOnFolderMock).toHaveBeenCalledWith(10);
    expect(context.reportStatus).toHaveBeenCalled();
    wrapper.unmount();
  });

  it("toggles run-on-arrival filtering for the account", async () => {
    const wrapper = await mountWith([]);
    await wrapper.get('input[aria-label="Run filters on new mail"]').trigger("change");
    await flushPromises();
    expect(setArrivalMock).toHaveBeenCalledWith(1, false);
    wrapper.unmount();
  });
});
