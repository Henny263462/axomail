import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  applyFolderRetention,
  getFolderRetention,
  setFolderRetention,
} from "../src/api/retention";
import FolderRetentionSection from "../src/components/offline/FolderRetentionSection.vue";

vi.mock("../src/api/retention", () => ({
  getFolderRetention: vi.fn(),
  setFolderRetention: vi.fn(),
  applyFolderRetention: vi.fn(),
}));

const getMock = vi.mocked(getFolderRetention);
const setMock = vi.mocked(setFolderRetention);
const applyMock = vi.mocked(applyFolderRetention);

beforeEach(() => {
  vi.clearAllMocks();
  getMock.mockResolvedValue({
    mailboxId: 10,
    mode: "keep_all",
    days: 30,
    maxCount: 1000,
    action: "trash",
  });
  setMock.mockResolvedValue(null);
  applyMock.mockResolvedValue({ movedToTrash: 2, purgedBodies: 0 });
});

describe("FolderRetentionSection", () => {
  it("loads the policy and saves an edited one", async () => {
    const wrapper = mount(FolderRetentionSection, { props: { mailboxId: 10 } });
    await flushPromises();
    expect(getMock).toHaveBeenCalledWith(10);
    expect(
      (wrapper.get('input[value="keep_all"]').element as HTMLInputElement)
        .checked,
    ).toBe(true);

    await wrapper.get('input[value="by_age"]').setValue();
    const save = wrapper.findAll("button").find((b) => b.text() === "Save");
    await save?.trigger("click");
    await flushPromises();

    expect(setMock).toHaveBeenCalledWith(10, {
      mode: "by_age",
      days: 30,
      maxCount: 1000,
      action: "trash",
    });
    expect(wrapper.text()).toContain("Retention policy saved.");
  });

  it("applies retention and reports the outcome", async () => {
    getMock.mockResolvedValue({
      mailboxId: 10,
      mode: "by_age",
      days: 30,
      maxCount: 1000,
      action: "trash",
    });
    const wrapper = mount(FolderRetentionSection, { props: { mailboxId: 10 } });
    await flushPromises();

    const apply = wrapper.findAll("button").find((b) => b.text() === "Apply now");
    await apply?.trigger("click");
    await flushPromises();

    expect(setMock).toHaveBeenCalled(); // persisted before applying
    expect(applyMock).toHaveBeenCalledWith(10);
    expect(wrapper.text()).toContain("2 moved to Trash");
  });

  it("shows an unavailable note in the browser preview", async () => {
    getMock.mockResolvedValue(null);
    const wrapper = mount(FolderRetentionSection, { props: { mailboxId: 10 } });
    await flushPromises();
    expect(wrapper.text()).toContain(
      "Retention needs the desktop app",
    );
  });
});
