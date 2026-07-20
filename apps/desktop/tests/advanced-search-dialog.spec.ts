import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createSavedSearch,
  runAdvancedSearch,
  updateSavedSearch,
} from "../src/api/savedSearch";
import AdvancedSearchDialog from "../src/components/search/AdvancedSearchDialog.vue";
import type { AccountSummary } from "../src/types/mail";
import type { SavedSearchDto, SearchQuery } from "../src/types/search";

vi.mock("../src/api/savedSearch", () => ({
  runAdvancedSearch: vi.fn(),
  createSavedSearch: vi.fn(),
  updateSavedSearch: vi.fn(),
}));

const runMock = vi.mocked(runAdvancedSearch);
const createMock = vi.mocked(createSavedSearch);
const updateMock = vi.mocked(updateSavedSearch);

const accounts: AccountSummary[] = [
  {
    id: 1,
    displayName: "Work",
    email: "klaus@theferbers.eu",
    authMechanism: "password",
    mailboxes: [{ id: 10, name: "Inbox", role: "inbox", unreadCount: 0 }],
  },
];

const query: SearchQuery = {
  matchType: "any",
  conditions: [{ field: "from", op: "contains", value: "ada" }],
  scope: { kind: "all_folders", mailboxId: null },
  sort: "date_asc",
};

function mountDialog(savedSearch: SavedSearchDto | null = null) {
  return mount(AdvancedSearchDialog, { props: { savedSearch, accounts } });
}

beforeEach(() => {
  runMock.mockReset();
  createMock.mockReset();
  updateMock.mockReset();
  runMock.mockResolvedValue([]);
  createMock.mockResolvedValue(null);
  updateMock.mockResolvedValue(null);
});

describe("AdvancedSearchDialog", () => {
  it("starts with one condition row and adds/removes rows", async () => {
    const wrapper = mountDialog();
    expect(wrapper.findAll('[aria-label="Condition field"]')).toHaveLength(1);

    await wrapper.find(".link").trigger("click");
    expect(wrapper.findAll('[aria-label="Condition field"]')).toHaveLength(2);

    await wrapper.findAll('[aria-label="Remove condition"]')[0]!.trigger("click");
    expect(wrapper.findAll('[aria-label="Condition field"]')).toHaveLength(1);
  });

  it("runs the built query and emits the results", async () => {
    runMock.mockResolvedValue([
      {
        messageId: 5,
        mailboxId: 10,
        accountId: 1,
        subject: "Quarterly invoice",
        fromName: "Ada",
        fromAddress: "ada@example.org",
        dateUnix: 2000,
        snippet: "…",
      },
    ]);
    const wrapper = mountDialog();
    await wrapper.find('input[aria-label="Condition value"]').setValue("invoice");
    await wrapper.findAll(".buttons .action")[0]!.trigger("click");
    await flushPromises();

    expect(runMock).toHaveBeenCalledWith({
      matchType: "all",
      conditions: [{ field: "subject", op: "contains", value: "invoice" }],
      scope: { kind: "all_folders", mailboxId: null },
      sort: "date_desc",
    });
    const run = wrapper.emitted("run");
    expect(run).toBeTruthy();
    expect(run![0]![0]).toMatchObject({ title: "", results: [{ messageId: 5 }] });
    expect(wrapper.emitted("close")).toBeTruthy();
  });

  // Finding 10: a null result is the browser-preview signal, not "no results".
  it("reports unavailable (not empty results) when the search returns null", async () => {
    runMock.mockResolvedValue(null);
    const wrapper = mountDialog();
    await wrapper.find('input[aria-label="Condition value"]').setValue("invoice");
    await wrapper.findAll(".buttons .action")[0]!.trigger("click");
    await flushPromises();

    // No empty result set is emitted, and the dialog stays open with a notice.
    expect(wrapper.emitted("run")).toBeFalsy();
    expect(wrapper.emitted("close")).toBeFalsy();
    expect(wrapper.find(".error").text()).toContain("browser preview");
  });

  it("saves a new search from the builder", async () => {
    const wrapper = mountDialog();
    await wrapper.find(".name-input").setValue("Invoices");
    await wrapper.find('input[aria-label="Condition value"]').setValue("invoice");
    await wrapper.findAll(".buttons .action")[1]!.trigger("click");
    await flushPromises();

    expect(createMock).toHaveBeenCalledWith({
      name: "Invoices",
      query: {
        matchType: "all",
        conditions: [{ field: "subject", op: "contains", value: "invoice" }],
        scope: { kind: "all_folders", mailboxId: null },
        sort: "date_desc",
      },
    });
    expect(wrapper.emitted("saved")).toBeTruthy();
  });

  it("prefills and updates an existing saved search", async () => {
    const saved: SavedSearchDto = { id: 7, name: "From Ada", query };
    const wrapper = mountDialog(saved);
    expect((wrapper.find(".name-input").element as HTMLInputElement).value).toBe(
      "From Ada",
    );
    expect(
      (wrapper.find('[aria-label="Condition field"]').element as HTMLSelectElement)
        .value,
    ).toBe("from");

    await wrapper.findAll(".buttons .action")[1]!.trigger("click");
    await flushPromises();
    expect(updateMock).toHaveBeenCalledWith(7, {
      name: "From Ada",
      query,
    });
  });
});
