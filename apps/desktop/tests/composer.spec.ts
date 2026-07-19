import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

const sendMessageMock = vi.mocked(sendMessage);
const saveDraftMock = vi.mocked(saveDraft);
const listIdentitiesMock = vi.mocked(listIdentities);

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

beforeEach(() => {
  vi.clearAllMocks();
  sendMessageMock.mockResolvedValue({ outboxId: 1, sent: true, error: null });
  saveDraftMock.mockResolvedValue(88);
  // No identities by default: the From picker stays hidden and the compose
  // payload carries no identityId, matching the pre-identity contract.
  listIdentitiesMock.mockResolvedValue(null);
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

describe("ComposerDialog identities", () => {
  it("preselects the default identity and sends under it", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, isDefault: true, email: "klaus@example.org" }),
      identity({ id: 8, email: "sales@example.org" }),
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    // The From picker is shown with the default selected.
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

    // The chosen identity carries its own account across.
    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ identityId: 8, accountId: 99 }),
    );
  });

  it("inserts the default identity's signature into a new message", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, isDefault: true, signatureText: "Cheers,\nKlaus" }),
    ]);
    const wrapper = mount(ComposerDialog, { props: { accountId: 42 } });
    await flushPromises();

    const body = wrapper.get("#compose-body").element as HTMLTextAreaElement;
    expect(body.value).toContain("-- \nCheers,\nKlaus");
  });

  it("replaces a draft's baked-in signature on identity switch, never duplicating", async () => {
    listIdentitiesMock.mockResolvedValue([
      identity({ id: 7, accountId: 3, isDefault: true, signatureText: "Klaus" }),
      identity({ id: 8, accountId: 3, signatureText: "Sales Team" }),
    ]);
    // The saved draft already carries the first identity's signature in its
    // body; the composer does not re-insert one for a loaded draft.
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

    await wrapper.get("#compose-from").setValue("8");

    const body = (wrapper.get("#compose-body").element as HTMLTextAreaElement)
      .value;
    // The new identity's signature replaced the old one in place.
    expect(body).toContain("-- \nSales Team");
    expect(body).not.toContain("Klaus");
    // Exactly one signature delimiter, and the user's body is preserved above it.
    expect(body.match(/-- \n/g)).toHaveLength(1);
    expect(body).toContain("Hello there");
  });
});

describe("ComposerDialog send later", () => {
  /** A datetime-local string roughly one hour in the future, in local time. */
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

    // Exactly one call, and it carries a schedule — never an immediate send
    // (an immediate send would omit scheduledAtUnix).
    expect(sendMessageMock).toHaveBeenCalledTimes(1);
    expect(sendMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ scheduledAtUnix: expect.any(Number) }),
    );
  });
});
