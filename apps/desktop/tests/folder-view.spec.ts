import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import SidebarFolderList from "../src/components/sidebar/SidebarFolderList.vue";
import SidebarPane from "../src/components/sidebar/SidebarPane.vue";
import SidebarViewSwitcher from "../src/components/sidebar/SidebarViewSwitcher.vue";
import { useFolderFavorites } from "../src/composables/useFolderFavorites";
import {
  useFolderView,
  type FolderViewMode,
} from "../src/composables/useFolderView";
import { useRecentFolders } from "../src/composables/useRecentFolders";
import type { AccountSummary } from "../src/types/mail";

// The api layers the sidebar's children touch resolve to null in a unit mount.
vi.mock("../src/lib/tauri", () => ({
  isTauri: () => false,
  safeInvoke: vi.fn().mockResolvedValue(null),
  invokeOrThrow: vi.fn().mockResolvedValue(null),
}));

// useFolderView exposes its setter through the composable only; wrap it for
// test setup (the composable is a singleton).
function setFolderMode(mode: FolderViewMode): void {
  useFolderView().setMode(mode);
}

const accounts: AccountSummary[] = [
  {
    id: 1,
    displayName: "Work",
    email: "klaus@theferbers.eu",
    authMechanism: "password",
    mailboxes: [
      { id: 10, name: "INBOX", role: "inbox", unreadCount: 3 },
      { id: 11, name: "INBOX/Projects", role: "custom", unreadCount: 0 },
    ],
  },
];

beforeEach(() => {
  localStorage.clear();
  setFolderMode("all");
});

describe("useFolderView", () => {
  it("persists the chosen mode", () => {
    const { mode, setMode: set } = useFolderView();
    set("favorite");
    expect(mode.value).toBe("favorite");
    expect(localStorage.getItem("axomail.folderView")).toBe("favorite");
  });
});

describe("useFolderFavorites", () => {
  it("toggles a folder in and out of favorites and persists it", () => {
    const { isFavorite, toggleFavorite, favoriteIds } = useFolderFavorites();
    const id = 4242;
    const before = isFavorite(id);
    toggleFavorite(id);
    expect(isFavorite(id)).toBe(!before);
    expect(favoriteIds.value).toContain(id);
    expect(
      JSON.parse(localStorage.getItem("axomail.favoriteFolders") ?? "[]"),
    ).toContain(id);
    toggleFavorite(id);
    expect(isFavorite(id)).toBe(before);
  });
});

describe("useRecentFolders", () => {
  it("records the newest folder first and de-duplicates", () => {
    const { recentIds, recordFolder } = useRecentFolders();
    recordFolder(1);
    recordFolder(2);
    recordFolder(1);
    expect(recentIds.value[0]).toBe(1);
    expect(recentIds.value.filter((id) => id === 1)).toHaveLength(1);
  });
});

describe("SidebarViewSwitcher", () => {
  it("renders every mode and marks the active one", async () => {
    const wrapper = mount(SidebarViewSwitcher, { props: { mode: "recent" } });
    const buttons = wrapper.findAll('[role="radio"]');
    expect(buttons).toHaveLength(5);
    const recent = buttons.find((b) => b.text() === "Recent");
    expect(recent?.attributes("aria-checked")).toBe("true");
    const favorite = buttons.find((b) => b.text() === "Favorite");
    await favorite?.trigger("click");
    expect(wrapper.emitted("select")?.at(-1)).toEqual(["favorite"]);
  });
});

describe("SidebarFolderList", () => {
  it("shows an empty state when there are no folders", () => {
    const wrapper = mount(SidebarFolderList, {
      props: {
        folders: [],
        selectedMailboxId: null,
        emptyTitle: "Nothing here",
        emptyDetail: "Add one.",
      },
    });
    expect(wrapper.text()).toContain("Nothing here");
  });

  it("renders folders and selects one", async () => {
    const wrapper = mount(SidebarFolderList, {
      props: {
        folders: [
          {
            mailbox: { id: 11, name: "INBOX/Projects", role: "custom", unreadCount: 5 },
            accountName: "Work",
          },
        ],
        selectedMailboxId: null,
        emptyTitle: "empty",
        emptyDetail: "empty",
      },
    });
    expect(wrapper.text()).toContain("Projects");
    expect(wrapper.text()).toContain("Work");
    expect(wrapper.text()).toContain("5");
    await wrapper.get("button.row").trigger("click");
    expect(wrapper.emitted("selectMailbox")?.at(-1)).toEqual([11]);
  });
});

describe("SidebarPane folder-view modes", () => {
  it("shows only the favorites list in favorite mode", async () => {
    useFolderFavorites().toggleFavorite(11);
    setFolderMode("favorite");
    const wrapper = mount(SidebarPane, {
      props: { accounts, selectedMailboxId: null },
    });
    // The favorites list shows the marked folder; the account tree is absent.
    expect(wrapper.text()).toContain("Projects");
    expect(wrapper.findComponent({ name: "FolderTree" }).exists()).toBe(false);

    // Selecting it records it as recent and bubbles the id up.
    await wrapper.get(".folder-list button.row").trigger("click");
    expect(wrapper.emitted("selectMailbox")?.at(-1)).toEqual([11]);
    expect(useRecentFolders().recentIds.value[0]).toBe(11);

    // Clean up the shared favorite so later tests start clean.
    useFolderFavorites().toggleFavorite(11);
  });

  it("shows only the All-inboxes entry in unified mode", () => {
    setFolderMode("unified");
    const wrapper = mount(SidebarPane, {
      props: { accounts, selectedMailboxId: null },
    });
    expect(wrapper.find("button.all-inboxes").exists()).toBe(true);
    expect(wrapper.findComponent({ name: "FolderTree" }).exists()).toBe(false);
  });
});
