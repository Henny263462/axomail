import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { deleteSavedSearch, listSavedSearches } from "../src/api/savedSearch";
import SidebarSavedSearchesPane from "../src/components/sidebar/SidebarSavedSearchesPane.vue";
import type { SavedSearchDto, SearchQuery } from "../src/types/search";

vi.mock("../src/api/savedSearch", () => ({
  listSavedSearches: vi.fn(),
  deleteSavedSearch: vi.fn(),
}));

const listMock = vi.mocked(listSavedSearches);
const deleteMock = vi.mocked(deleteSavedSearch);

const query: SearchQuery = {
  matchType: "all",
  conditions: [],
  scope: { kind: "all_folders", mailboxId: null },
  sort: "date_desc",
};

const saved: SavedSearchDto[] = [
  { id: 1, name: "Invoices", query },
  { id: 2, name: "Unread", query },
];

beforeEach(() => {
  listMock.mockReset();
  deleteMock.mockReset();
  deleteMock.mockResolvedValue(null);
});

describe("SidebarSavedSearchesPane", () => {
  it("renders no group when there are no saved searches", async () => {
    listMock.mockResolvedValue([]);
    const wrapper = mount(SidebarSavedSearchesPane);
    await flushPromises();
    expect(wrapper.find(".saved-group").exists()).toBe(false);
  });

  it("lists saved searches and emits select on click", async () => {
    listMock.mockResolvedValue(saved);
    const wrapper = mount(SidebarSavedSearchesPane);
    await flushPromises();

    const items = wrapper.findAll(".saved");
    expect(items).toHaveLength(2);
    expect(items[0]!.text()).toContain("Invoices");

    await items[0]!.trigger("click");
    expect(wrapper.emitted("select")![0]![0]).toMatchObject({ id: 1 });
  });

  it("emits edit for a saved search", async () => {
    listMock.mockResolvedValue(saved);
    const wrapper = mount(SidebarSavedSearchesPane);
    await flushPromises();

    const edit = wrapper
      .findAll("button")
      .find((b) => b.attributes("aria-label")?.startsWith("Edit saved search"));
    await edit?.trigger("click");
    expect(wrapper.emitted("edit")![0]![0]).toMatchObject({ id: 1 });
  });

  it("deletes a saved search, reloads, and emits changed", async () => {
    listMock.mockResolvedValueOnce(saved).mockResolvedValueOnce([saved[1]!]);
    const wrapper = mount(SidebarSavedSearchesPane);
    await flushPromises();

    const del = wrapper
      .findAll("button")
      .find((b) =>
        b.attributes("aria-label")?.startsWith("Delete saved search: Invoices"),
      );
    await del?.trigger("click");
    await flushPromises();

    expect(deleteMock).toHaveBeenCalledWith(1);
    expect(wrapper.emitted("changed")).toBeTruthy();
    expect(wrapper.findAll(".saved")).toHaveLength(1);
    expect(wrapper.find(".saved").text()).toContain("Unread");
  });
});
