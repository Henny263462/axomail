import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  getMessageHeaders,
  getMessageSource,
  pickDirectoryPath,
  pickSaveFilePath,
  saveAllAttachments,
  saveAttachment,
  saveMessageEml,
} from "../src/api/attachments";
import MessageActions from "../src/components/message/MessageActions.vue";
import MessageAttachments from "../src/components/message/MessageAttachments.vue";
import type { AttachmentMetaDto } from "../src/types/mail";

vi.mock("../src/api/attachments", () => ({
  pickSaveFilePath: vi.fn(),
  pickDirectoryPath: vi.fn(),
  saveAttachment: vi.fn(),
  saveAllAttachments: vi.fn(),
  getMessageSource: vi.fn(),
  getMessageHeaders: vi.fn(),
  saveMessageEml: vi.fn(),
}));

const pickSaveFilePathMock = vi.mocked(pickSaveFilePath);
const pickDirectoryPathMock = vi.mocked(pickDirectoryPath);
const saveAttachmentMock = vi.mocked(saveAttachment);
const saveAllAttachmentsMock = vi.mocked(saveAllAttachments);
const getMessageSourceMock = vi.mocked(getMessageSource);
const getMessageHeadersMock = vi.mocked(getMessageHeaders);
const saveMessageEmlMock = vi.mocked(saveMessageEml);

function attachment(over: Partial<AttachmentMetaDto> = {}): AttachmentMetaDto {
  return { filename: "report.pdf", mimeType: "application/pdf", size: 2048, ...over };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("MessageAttachments", () => {
  it("saves one attachment to the dialog-chosen path", async () => {
    pickSaveFilePathMock.mockResolvedValue("/home/k/report.pdf");
    saveAttachmentMock.mockResolvedValue({ filename: "report.pdf", bytes: 2048 });
    const wrapper = mount(MessageAttachments, {
      props: { messageId: 42, attachments: [attachment()] },
    });

    await wrapper.get('button[aria-label="Save report.pdf"]').trigger("click");
    await flushPromises();

    expect(pickSaveFilePathMock).toHaveBeenCalledWith("report.pdf");
    expect(saveAttachmentMock).toHaveBeenCalledWith(42, 0, "/home/k/report.pdf");
    expect(wrapper.text()).toContain("Saved report.pdf.");
  });

  it("does not save when the save dialog is cancelled", async () => {
    pickSaveFilePathMock.mockResolvedValue(null);
    const wrapper = mount(MessageAttachments, {
      props: { messageId: 42, attachments: [attachment()] },
    });

    await wrapper.get('button[aria-label="Save report.pdf"]').trigger("click");
    await flushPromises();

    expect(saveAttachmentMock).not.toHaveBeenCalled();
  });

  it("offers Save all only for multiple attachments and picks a directory", async () => {
    pickDirectoryPathMock.mockResolvedValue("/home/k/downloads");
    saveAllAttachmentsMock.mockResolvedValue([
      { filename: "a.pdf", bytes: 1 },
      { filename: "b.pdf", bytes: 2 },
    ]);
    const single = mount(MessageAttachments, {
      props: { messageId: 9, attachments: [attachment()] },
    });
    expect(single.text()).not.toContain("Save all");

    const many = mount(MessageAttachments, {
      props: {
        messageId: 9,
        attachments: [attachment({ filename: "a.pdf" }), attachment({ filename: "b.pdf" })],
      },
    });
    const saveAll = many.findAll("button").find((b) => b.text() === "Save all");
    if (saveAll === undefined) throw new Error("Save all button missing");
    await saveAll.trigger("click");
    await flushPromises();

    expect(pickDirectoryPathMock).toHaveBeenCalled();
    expect(saveAllAttachmentsMock).toHaveBeenCalledWith(9, "/home/k/downloads");
    expect(many.text()).toContain("Saved 2 attachments.");
  });
});

describe("MessageActions", () => {
  it("toggles the full header block, fetching it once", async () => {
    getMessageHeadersMock.mockResolvedValue("Received: from mx\r\nMessage-ID: <a@b>");
    const wrapper = mount(MessageActions, { props: { messageId: 7, subject: "Hi" } });

    const button = wrapper.findAll("button").find((b) => b.text() === "All headers");
    if (button === undefined) throw new Error("All headers button missing");
    await button.trigger("click");
    await flushPromises();

    expect(getMessageHeadersMock).toHaveBeenCalledWith(7);
    expect(wrapper.get('[aria-label="Full message headers"]').text()).toContain("Message-ID: <a@b>");
    expect(wrapper.text()).toContain("Hide headers");

    // Toggling off hides the block and does not re-fetch.
    await wrapper.findAll("button").find((b) => b.text() === "Hide headers")?.trigger("click");
    await flushPromises();
    expect(wrapper.find('[aria-label="Full message headers"]').exists()).toBe(false);
    expect(getMessageHeadersMock).toHaveBeenCalledTimes(1);
  });

  it("opens a read-only source dialog with the raw message", async () => {
    getMessageSourceMock.mockResolvedValue("From: a@b\r\nSubject: Hi\r\n\r\nbody");
    const wrapper = mount(MessageActions, { props: { messageId: 7, subject: "Hi" } });

    await wrapper.findAll("button").find((b) => b.text() === "View source")?.trigger("click");
    await flushPromises();

    expect(getMessageSourceMock).toHaveBeenCalledWith(7);
    const dialog = wrapper.get('[role="dialog"]');
    expect(dialog.text()).toContain("Message source — Hi");
    expect(wrapper.get('[aria-label="Raw message source"]').text()).toContain("Subject: Hi");
  });

  it("saves the message as .eml through the save dialog", async () => {
    pickSaveFilePathMock.mockResolvedValue("/home/k/Hi.eml");
    saveMessageEmlMock.mockResolvedValue(120);
    const wrapper = mount(MessageActions, { props: { messageId: 7, subject: "Hi" } });

    await wrapper.findAll("button").find((b) => b.text() === "Save as .eml")?.trigger("click");
    await flushPromises();

    expect(pickSaveFilePathMock).toHaveBeenCalledWith("Hi.eml", [
      { name: "Email message", extensions: ["eml"] },
    ]);
    expect(saveMessageEmlMock).toHaveBeenCalledWith(7, "/home/k/Hi.eml");
    expect(wrapper.text()).toContain("Saved message as .eml.");
  });

  it("resets cached header/source state when the message changes", async () => {
    getMessageHeadersMock.mockResolvedValue("Message-ID: <first@x>");
    const wrapper = mount(MessageActions, { props: { messageId: 1, subject: "A" } });
    await wrapper.findAll("button").find((b) => b.text() === "All headers")?.trigger("click");
    await flushPromises();
    expect(wrapper.find('[aria-label="Full message headers"]').exists()).toBe(true);

    await wrapper.setProps({ messageId: 2, subject: "B" });
    await flushPromises();
    // Switching messages collapses the header block (no stale headers shown).
    expect(wrapper.find('[aria-label="Full message headers"]').exists()).toBe(false);
  });
});
