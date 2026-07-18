import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { saveDraft, sendMessage } from "../src/api/compose";
import ComposerDialog from "../src/components/compose/ComposerDialog.vue";
import type { DraftDto } from "../src/types/mail";

vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
}));

const sendMessageMock = vi.mocked(sendMessage);
const saveDraftMock = vi.mocked(saveDraft);

function saveDraftButton(wrapper: VueWrapper) {
  const button = wrapper
    .findAll("button")
    .find((candidate) => candidate.text().startsWith("Save draft"));
  if (button === undefined) throw new Error("save-draft button missing");
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  sendMessageMock.mockResolvedValue({ outboxId: 1, sent: true, error: null });
  saveDraftMock.mockResolvedValue(88);
});

describe("ComposerDialog send", () => {
  it("splits recipients and submits the exact ComposeInput", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });

    await wrapper.get("#compose-to").setValue("a@x.com, b@y.com ");
    await wrapper.get("#compose-cc").setValue(" c@z.com");
    await wrapper.get("#compose-subject").setValue(" Hi there ");
    await wrapper.get("#compose-body").setValue("Hello world");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith({
      accountId: 42,
      to: ["a@x.com", "b@y.com"],
      cc: ["c@z.com"],
      bcc: [],
      subject: "Hi there",
      textBody: "Hello world",
    });
    expect(wrapper.emitted("sent")?.[0]).toEqual([{ draftId: null }]);
  });

  it("blocks sending without a recipient", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await wrapper.get("#compose-subject").setValue("No recipients");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "Add at least one recipient.",
    );
  });

  it("surfaces the backend error and stays open when not sent", async () => {
    sendMessageMock.mockResolvedValue({
      outboxId: 5,
      sent: false,
      error: "SMTP server refused the message.",
    });
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "SMTP server refused the message.",
    );
    expect(wrapper.emitted("sent")).toBeUndefined();
  });
});

describe("ComposerDialog drafts", () => {
  const draft: DraftDto = {
    id: 88,
    accountId: 3,
    to: ["draft@example.org"],
    cc: [],
    bcc: [],
    subject: "Saved earlier",
    textBody: "Half-written body",
    updatedUnix: 1_752_790_000,
  };

  it("prefills from a draft and saves against its id", async () => {
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 42, draft },
    });
    // Prefill happens in onMounted; let the bound inputs update.
    await flushPromises();

    expect((wrapper.get("#compose-to").element as HTMLInputElement).value).toBe(
      "draft@example.org",
    );
    expect(
      (wrapper.get("#compose-subject").element as HTMLInputElement).value,
    ).toBe("Saved earlier");

    await saveDraftButton(wrapper).trigger("click");
    await flushPromises();

    expect(saveDraftMock).toHaveBeenCalledWith(
      {
        accountId: 3,
        to: ["draft@example.org"],
        cc: [],
        bcc: [],
        subject: "Saved earlier",
        textBody: "Half-written body",
      },
      88,
    );
    expect(wrapper.emitted("saved")?.[0]).toEqual([88]);
    expect(wrapper.get('[role="status"]').text()).toContain("Draft saved.");
  });

  it("creates a new draft with a null id from a blank composer", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 7 } });
    await wrapper.get("#compose-to").setValue("new@example.org");
    await saveDraftButton(wrapper).trigger("click");
    await flushPromises();

    expect(saveDraftMock).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 7, to: ["new@example.org"] }),
      null,
    );
  });
});
