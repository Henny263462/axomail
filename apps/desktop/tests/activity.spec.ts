import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { cancelAllSyncJobs, cancelSyncJob, listSyncJobs } from "../src/api/jobs";
import { subscribeSyncEvents } from "../src/api/sync";
import ActivityDialog from "../src/components/activity/ActivityDialog.vue";
import type { SyncJobDto } from "../src/types/mail";

vi.mock("../src/api/jobs", () => ({
  listSyncJobs: vi.fn(),
  cancelSyncJob: vi.fn(),
  cancelAllSyncJobs: vi.fn(),
}));
vi.mock("../src/api/sync", () => ({
  subscribeSyncEvents: vi.fn(),
}));

const listSyncJobsMock = vi.mocked(listSyncJobs);
const cancelSyncJobMock = vi.mocked(cancelSyncJob);
const cancelAllSyncJobsMock = vi.mocked(cancelAllSyncJobs);
const subscribeSyncEventsMock = vi.mocked(subscribeSyncEvents);

function job(overrides: Partial<SyncJobDto>): SyncJobDto {
  return {
    id: 1,
    kind: "mailboxSync",
    state: "running",
    accountId: 1,
    accountLabel: "Work",
    mailboxId: 10,
    mailboxLabel: "Inbox",
    error: null,
    startedAtUnix: null,
    finishedAtUnix: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  cancelSyncJobMock.mockResolvedValue(null);
  cancelAllSyncJobsMock.mockResolvedValue(null);
  subscribeSyncEventsMock.mockResolvedValue(() => {});
});

describe("ActivityDialog", () => {
  it("renders active and recent jobs with context and a cancel-all button", async () => {
    listSyncJobsMock.mockResolvedValue([
      job({ id: 1, state: "running", mailboxLabel: "Inbox" }),
      job({ id: 2, state: "pending", mailboxLabel: "Archive" }),
      job({ id: 3, state: "failed", mailboxLabel: "Sent", error: "boom" }),
    ]);

    const wrapper = mount(ActivityDialog);
    await flushPromises();

    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.attributes("aria-labelledby")).toBe("activity-title");
    // Every job's mailbox context is shown, plus the redacted failure reason.
    expect(wrapper.text()).toContain("Inbox");
    expect(wrapper.text()).toContain("Archive");
    expect(wrapper.text()).toContain("Sent");
    expect(wrapper.text()).toContain("boom");
    // State badges use human labels.
    expect(wrapper.text()).toContain("Running");
    expect(wrapper.text()).toContain("Queued");
    expect(wrapper.text()).toContain("Failed");
    // Two active jobs → a cancel-all affordance is offered.
    expect(
      wrapper.find('[aria-label="Cancel all sync jobs"]').exists(),
    ).toBe(true);
  });

  it("cancels a single job and reloads the list", async () => {
    listSyncJobsMock.mockResolvedValue([
      job({ id: 7, state: "running", mailboxLabel: "Inbox" }),
    ]);

    const wrapper = mount(ActivityDialog);
    await flushPromises();
    expect(listSyncJobsMock).toHaveBeenCalledTimes(1);

    await wrapper.get('[aria-label="Cancel Inbox"]').trigger("click");
    await flushPromises();

    expect(cancelSyncJobMock).toHaveBeenCalledWith(7);
    // The list is reloaded after cancelling.
    expect(listSyncJobsMock).toHaveBeenCalledTimes(2);
  });

  it("cancels every job from the cancel-all button", async () => {
    listSyncJobsMock.mockResolvedValue([job({ id: 1, state: "running" })]);

    const wrapper = mount(ActivityDialog);
    await flushPromises();

    await wrapper.get('[aria-label="Cancel all sync jobs"]').trigger("click");
    await flushPromises();

    expect(cancelAllSyncJobsMock).toHaveBeenCalledTimes(1);
  });

  it("shows an honest empty state when there are no jobs", async () => {
    listSyncJobsMock.mockResolvedValue([]);

    const wrapper = mount(ActivityDialog);
    await flushPromises();

    expect(wrapper.text()).toContain("No background activity");
    expect(
      wrapper.find('[aria-label="Cancel all sync jobs"]').exists(),
    ).toBe(false);
  });

  it("explains the browser-preview limitation when unavailable", async () => {
    listSyncJobsMock.mockResolvedValue(null);

    const wrapper = mount(ActivityDialog);
    await flushPromises();

    expect(wrapper.text()).toContain("unavailable in the browser preview");
  });

  it("emits close from the Close button", async () => {
    listSyncJobsMock.mockResolvedValue([]);

    const wrapper = mount(ActivityDialog);
    await flushPromises();

    await wrapper.get("button.button").trigger("click");
    expect(wrapper.emitted("close")).toHaveLength(1);
  });
});
