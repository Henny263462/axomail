import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { onFilesDropped, pickAttachments, statAttachments } from "../src/api/attachments";
import { saveDraft, sendMessage } from "../src/api/compose";
import { listIdentities } from "../src/api/identities";
import ComposerDialog from "../src/components/compose/ComposerDialog.vue";
import type { DraftDto, IdentityDto } from "../src/types/mail";

vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
}));
vi.mock("../src/api/identities", () => ({
  listIdentities: vi.fn(),
}));
vi.mock("../src/api/attachments", () => ({
  pickAttachments: vi.fn(),
  statAttachments: vi.fn(),
  onFilesDropped: vi.fn(() => () => {}),
}));

const sendMessageMock = vi.mocked(sendMessage);
const saveDraftMock = vi.mocked(saveDraft);
const listIdentitiesMock = vi.mocked(listIdentities);
const pickAttachmentsMock = vi.mocked(pickAttachments);
const statAttachmentsMock = vi.mocked(statAttachments);
const onFilesDroppedMock = vi.mocked(onFilesDropped);

/** Builds an identity DTO with sensible defaults for the picker tests. */
function identity(over: Partial<IdentityDto>): IdentityDto {
  return {
    id: 1,
    accountId: 42,
    email: "klaus@example.org",
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

function saveDraftButton(wrapper: VueWrapper) {
  const button = wrapper
    .findAll("button")
    .find((candidate) => candidate.text().startsWith("Save draft"));
  if (button === undefined) throw new Error("save-draft button missing");
  return button;
}

function attachButton(wrapper: VueWrapper) {
  const button = wrapper
    .findAll("button")
    .find((candidate) => candidate.text().includes("Attach files"));
  if (button === undefined) throw new Error("attach-files button missing");
  return button;
}

/** Switches the body editor to the plain-text surface (a testable textarea). */
async function usePlainBody(wrapper: VueWrapper): Promise<void> {
  await wrapper.get(".mode-toggle").trigger("click");
}

beforeEach(() => {
  vi.clearAllMocks();
  sendMessageMock.mockResolvedValue({ outboxId: 1, sent: true, error: null });
  saveDraftMock.mockResolvedValue(88);
  listIdentitiesMock.mockResolvedValue(null);
  onFilesDroppedMock.mockReturnValue(() => {});
});

describe("ComposerDialog send", () => {
  it("splits recipients and submits html + attachment fields", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
    await usePlainBody(wrapper);

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
      htmlBody: null,
      attachments: [],
    });
    expect(wrapper.emitted("sent")?.[0]).toEqual([{ draftId: null }]);
  });

  it("sends a sanitized HTML body from the rich editor", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    await wrapper.get("#compose-to").setValue("a@x.com");
    const editor = wrapper.get("#compose-editor");
    (editor.element as HTMLElement).innerHTML =
      "<p>Hello <b>bold</b></p><script>alert(1)</script>";
    await editor.trigger("input");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    const payload = sendMessageMock.mock.calls[0]?.[0];
    expect(payload?.htmlBody).toContain("<b>bold</b>");
    expect(payload?.htmlBody).not.toContain("script");
    expect(payload?.textBody).toContain("Hello bold");
  });

  it("blocks sending without a recipient", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
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
    await flushPromises();
    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "SMTP server refused the message.",
    );
    expect(wrapper.emitted("sent")).toBeUndefined();
  });
});

describe("ComposerDialog attachments", () => {
  it("adds a picked attachment and includes its path on send", async () => {
    pickAttachmentsMock.mockResolvedValue(["/home/k/report.pdf"]);
    statAttachmentsMock.mockResolvedValue([
      { path: "/home/k/report.pdf", filename: "report.pdf", sizeBytes: 2048 },
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    await wrapper.get("#compose-to").setValue("a@x.com");
    await attachButton(wrapper).trigger("click");
    await flushPromises();

    // A chip appears for the picked file.
    expect(wrapper.text()).toContain("report.pdf");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    const payload = sendMessageMock.mock.calls[0]?.[0];
    expect(payload?.attachments).toEqual(["/home/k/report.pdf"]);
  });

  it("warns once when the body mentions an attachment but none is attached", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
    await usePlainBody(wrapper);
    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("#compose-body").setValue("The file is attached below.");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    // First send is intercepted by the reminder.
    expect(sendMessageMock).not.toHaveBeenCalled();
    expect(wrapper.get(".warning").text()).toContain("none is attached");

    // Sending again honours the warning.
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(sendMessageMock).toHaveBeenCalledTimes(1);
  });

  it("does not warn when a file is already attached", async () => {
    pickAttachmentsMock.mockResolvedValue(["/home/k/a.pdf"]);
    statAttachmentsMock.mockResolvedValue([
      { path: "/home/k/a.pdf", filename: "a.pdf", sizeBytes: 10 },
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
    await usePlainBody(wrapper);
    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("#compose-body").setValue("See attached.");
    await attachButton(wrapper).trigger("click");
    await flushPromises();

    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(sendMessageMock).toHaveBeenCalledTimes(1);
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
    await flushPromises();

    expect((wrapper.get("#compose-to").element as HTMLInputElement).value).toBe(
      "draft@example.org",
    );
    expect(
      (wrapper.get("#compose-subject").element as HTMLInputElement).value,
    ).toBe("Saved earlier");
    // A draft opens in plain-text mode, so its textarea is present.
    expect(
      (wrapper.get("#compose-body").element as HTMLTextAreaElement).value,
    ).toBe("Half-written body");

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
        htmlBody: null,
        attachments: [],
      },
      88,
    );
    expect(wrapper.emitted("saved")?.[0]).toEqual([88]);
    expect(wrapper.get('[role="status"]').text()).toContain("Draft saved.");
  });

  it("creates a new draft with a null id from a blank composer", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 7 } });
    await flushPromises();
    await wrapper.get("#compose-to").setValue("new@example.org");
    await saveDraftButton(wrapper).trigger("click");
    await flushPromises();

    expect(saveDraftMock).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 7, to: ["new@example.org"] }),
      null,
    );
  });
});

describe("ComposerDialog identities", () => {
  it("preselects the default identity and sends under it", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, isDefault: true, email: "klaus@example.org" }),
      identity({ id: 8, email: "sales@example.org" }),
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    expect(
      (wrapper.get("#compose-from").element as HTMLSelectElement).value,
    ).toBe("7");

    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 42, identityId: 7 }),
    );
  });

  it("switches identity through the From picker", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, isDefault: true }),
      identity({ id: 8, accountId: 99, email: "other@example.org" }),
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    await wrapper.get("#compose-from").setValue("8");
    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ identityId: 8, accountId: 99 }),
    );
  });

  it("appends the default identity's signature to the sent body", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, isDefault: true, signatureText: "Cheers,\nKlaus" }),
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    // The signature is previewed, not baked into the editable body.
    expect(wrapper.get(".signature-body").text()).toContain("Cheers,");

    await wrapper.get("#compose-to").setValue("a@x.com");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    const payload = sendMessageMock.mock.calls[0]?.[0];
    expect(payload?.textBody).toContain("-- \nCheers,\nKlaus");
  });

  it("swaps the previewed signature on identity switch", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, accountId: 3, isDefault: true, signatureText: "Klaus" }),
      identity({ id: 8, accountId: 3, signatureText: "Sales Team" }),
    ]);
    const draftWithSig: DraftDto = {
      id: 88,
      accountId: 3,
      to: ["x@y.com"],
      cc: [],
      bcc: [],
      subject: "Re: hi",
      textBody: "Hello there\n\n-- \nKlaus",
      updatedUnix: 1_752_790_000,
    };
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 3, draft: draftWithSig },
    });
    await flushPromises();

    // The baked-in signature was stripped from the editable body on load.
    expect(
      (wrapper.get("#compose-body").element as HTMLTextAreaElement).value,
    ).toBe("Hello there");

    await wrapper.get("#compose-from").setValue("8");
    const preview = wrapper.get(".signature-body").text();
    expect(preview).toContain("Sales Team");
    expect(preview).not.toContain("Klaus");
  });
});

describe("ComposerDialog send later", () => {
  function futureLocal(): string {
    const d = new Date(Date.now() + 3_600_000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
      d.getHours(),
    )}:${pad(d.getMinutes())}`;
  }

  it("schedules the message through the send-later menu", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
    await wrapper.get("#compose-to").setValue("a@x.com");

    await wrapper.get('button[aria-label="Send later"]').trigger("click");
    await wrapper.findAll("button.menu-item")[0]?.trigger("click");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ scheduledAtUnix: expect.any(Number) }),
    );
    expect(wrapper.emitted("sent")).toBeTruthy();
  });

  it("Enter in the datetime field schedules rather than sending now", async () => {
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();
    await wrapper.get("#compose-to").setValue("a@x.com");

    await wrapper.get('button[aria-label="Send later"]').trigger("click");
    await wrapper.get("#send-later-custom").setValue(futureLocal());
    await wrapper.get("#send-later-custom").trigger("keydown", { key: "Enter" });
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledTimes(1);
    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ scheduledAtUnix: expect.any(Number) }),
    );
  });
});
