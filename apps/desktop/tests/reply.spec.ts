import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { saveDraft, sendMessage } from "../src/api/compose";
import { listIdentities } from "../src/api/identities";
import { getMessageDetail } from "../src/api/messages";
import { markReplySource } from "../src/api/reply";
import ComposerDialog from "../src/components/compose/ComposerDialog.vue";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import type {
  ComposerPrefillDto,
  IdentityDto,
  MessageDetailDto,
} from "../src/types/mail";

vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
}));
vi.mock("../src/api/identities", () => ({ listIdentities: vi.fn() }));
vi.mock("../src/api/reply", () => ({ markReplySource: vi.fn() }));
vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
  setMessageFlagged: vi.fn(),
}));

const sendMessageMock = vi.mocked(sendMessage);
const saveDraftMock = vi.mocked(saveDraft);
const listIdentitiesMock = vi.mocked(listIdentities);
const markReplySourceMock = vi.mocked(markReplySource);
const getMessageDetailMock = vi.mocked(getMessageDetail);

function identity(over: Partial<IdentityDto>): IdentityDto {
  return {
    id: 7,
    accountId: 42,
    email: "klaus@example.org",
    displayName: "",
    replyTo: null,
    signatureText: null,
    signaturePlacement: "below_quote",
    replyPosition: "below",
    forwardMode: "inline",
    archiveGranularity: "flat",
    isDefault: true,
    sortPosition: 0,
    sentDest: null,
    draftsDest: null,
    templatesDest: null,
    archiveDest: null,
    ...over,
  };
}

const replyPrefill: ComposerPrefillDto = {
  accountId: 42,
  identityId: 7,
  to: ["Ada Lovelace <ada@example.org>"],
  cc: ["Carol <carol@example.net>"],
  subject: "Re: Weekly sync",
  body: "\n\nOn 2020-09-13 12:26 UTC, Ada Lovelace <ada@example.org> wrote:\n> Original line.",
  inReplyTo: "parent-1@example.org",
  references: ["root-0@example.org", "parent-1@example.org"],
  attachments: [],
  source: { messageId: 501, mark: "answered" },
};

const forwardPrefill: ComposerPrefillDto = {
  accountId: 42,
  identityId: 7,
  to: [],
  cc: [],
  subject: "Fwd: Weekly sync",
  body: "\n\n-- \nKlaus",
  inReplyTo: null,
  references: [],
  attachments: [
    {
      filename: "Weekly sync.eml",
      mimeType: "message/rfc822",
      contentBase64: "RnJvbTogYWRhQGV4YW1wbGUub3Jn",
    },
  ],
  source: { messageId: 501, mark: "forwarded" },
};

const detail: MessageDetailDto = {
  id: 501,
  subject: "Weekly sync",
  fromName: "Ada Lovelace",
  fromAddress: "ada@example.org",
  to: [{ name: "Klaus", address: "klaus@theferbers.eu" }],
  cc: [],
  dateUnix: 1_600_000_000,
  flagged: false,
  textBody: "Original line.",
  sanitizedHtml: null,
  remoteBlockedCount: 0,
  remoteAllowed: false,
  scamFlags: [],
  hadActiveContent: false,
  attachments: [],
  messageId: "parent-1@example.org",
  inReplyTo: [],
  references: [],
};

function button(wrapper: VueWrapper, label: string) {
  const found = wrapper
    .findAll("button")
    .find((candidate) => candidate.text() === label);
  if (found === undefined) throw new Error(`button "${label}" not found`);
  return found;
}

beforeEach(() => {
  vi.clearAllMocks();
  sendMessageMock.mockResolvedValue({ outboxId: 1, sent: true, error: null });
  saveDraftMock.mockResolvedValue(88);
  markReplySourceMock.mockResolvedValue(null);
  listIdentitiesMock.mockResolvedValue([identity({ id: 7 })]);
  getMessageDetailMock.mockResolvedValue(detail);
});

describe("MessageDetail action bar", () => {
  it("emits reply / reply-all / forward with the message id", async () => {
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    await button(wrapper, "Reply").trigger("click");
    await button(wrapper, "Reply all").trigger("click");
    await button(wrapper, "Forward").trigger("click");

    expect(wrapper.emitted("reply")?.[0]).toEqual([501]);
    expect(wrapper.emitted("replyAll")?.[0]).toEqual([501]);
    expect(wrapper.emitted("forward")?.[0]).toEqual([501]);
  });
});

describe("ComposerDialog reply prefill", () => {
  it("prefills recipients, subject, and quoted body", async () => {
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 42, prefill: replyPrefill },
    });
    await flushPromises();

    expect((wrapper.get("#compose-to").element as HTMLInputElement).value).toBe(
      "Ada Lovelace <ada@example.org>",
    );
    expect((wrapper.get("#compose-cc").element as HTMLInputElement).value).toBe(
      "Carol <carol@example.net>",
    );
    expect(
      (wrapper.get("#compose-subject").element as HTMLInputElement).value,
    ).toBe("Re: Weekly sync");
    // The rich editor (contenteditable) holds the quoted body.
    expect(wrapper.get("#compose-editor").element.textContent).toContain(
      "> Original line.",
    );
    // The title reflects the reply mode.
    expect(wrapper.get("#composer-title").text()).toBe("Reply");
  });

  it("sends with the threading headers and marks the source answered", async () => {
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 42, prefill: replyPrefill },
    });
    await flushPromises();

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        to: ["Ada Lovelace <ada@example.org>"],
        inReplyTo: "parent-1@example.org",
        references: ["root-0@example.org", "parent-1@example.org"],
        identityId: 7,
      }),
    );
    // Marked only after the send is confirmed delivered.
    expect(markReplySourceMock).toHaveBeenCalledWith(501, "answered");
    expect(wrapper.emitted("sent")).toBeTruthy();
  });

  it("does not mark the source when the send is queued for later", async () => {
    sendMessageMock.mockResolvedValue({
      outboxId: 2,
      sent: false,
      error: null,
      scheduledAtUnix: 2_000_000_000,
    });
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 42, prefill: replyPrefill },
    });
    await flushPromises();

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalled();
    // A message that did not leave now must not mark its source answered.
    expect(markReplySourceMock).not.toHaveBeenCalled();
    expect(wrapper.emitted("sent")).toBeTruthy();
  });
});

describe("ComposerDialog forward prefill", () => {
  it("carries the forwarded attachment and marks the source forwarded", async () => {
    const wrapper = mount(ComposerDialog, {
      props: { accountId: 42, prefill: forwardPrefill },
    });
    await flushPromises();

    expect(wrapper.get("#composer-title").text()).toBe("Forward");
    // A forward starts with no recipients; the user addresses it.
    await wrapper.get("#compose-to").setValue("dave@example.org");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({
        to: ["dave@example.org"],
        forwardedAttachments: [
          {
            filename: "Weekly sync.eml",
            mimeType: "message/rfc822",
            contentBase64: "RnJvbTogYWRhQGV4YW1wbGUub3Jn",
          },
        ],
      }),
    );
    // A forward does not add threading headers.
    expect(sendMessageMock.mock.calls[0]?.[0].inReplyTo).toBeUndefined();
    expect(markReplySourceMock).toHaveBeenCalledWith(501, "forwarded");
  });
});
