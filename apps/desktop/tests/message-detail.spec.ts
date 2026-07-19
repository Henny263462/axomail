import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getMessageDetail } from "../src/api/messages";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import type { MessageDetailDto } from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
}));

const getMessageDetailMock = vi.mocked(getMessageDetail);

const htmlDetail: MessageDetailDto = {
  id: 501,
  subject: "Quarterly update",
  fromName: "Ada Lovelace",
  fromAddress: "ada@example.org",
  to: [{ name: "Klaus", address: "klaus@theferbers.eu" }],
  cc: [{ name: null, address: "cc@example.org" }],
  dateUnix: 1_752_790_000,
  textBody: "Plain text version of the update.",
  sanitizedHtml: '<p>Hello <a href="https://example.org">link</a></p>',
  remoteBlocked: 3,
  hadActiveContent: true,
  attachments: [
    { filename: "report.pdf", mimeType: "application/pdf", size: 20_480 },
  ],
  messageId: "quarterly-1@example.org",
  inReplyTo: [],
  references: [],
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("MessageViewPane detail", () => {
  it("shows a loading state then renders the header and notices", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });

    expect(wrapper.text()).toContain("Loading message…");

    await flushPromises();

    expect(getMessageDetailMock).toHaveBeenCalledWith(501);
    expect(wrapper.text()).toContain("Quarterly update");
    expect(wrapper.text()).toContain("klaus@theferbers.eu");
    expect(wrapper.text()).toContain("3 remote resources blocked");
    expect(wrapper.text()).toContain("Active content was removed");
    expect(wrapper.text()).toContain("report.pdf");
    expect(wrapper.text()).toContain("20 KB");
  });

  it("renders sanitized html in a locked-down sandboxed iframe by default", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const iframe = wrapper.get("iframe");
    // No allow-scripts, no allow-same-origin (ADR-0007): empty sandbox.
    expect(iframe.attributes("sandbox")).toBe("");
    expect(iframe.attributes("srcdoc")).toContain("<p>Hello");
    expect(iframe.attributes("srcdoc")).toContain("<a ");
    expect(wrapper.text()).toContain("Links are disabled in this preview");
  });

  it("switches to the always-available text view", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const textTab = wrapper
      .findAll("button.tab")
      .find((tab) => tab.text() === "Text");
    if (textTab === undefined) throw new Error("text tab missing");
    await textTab.trigger("click");

    expect(wrapper.get("pre.text").text()).toContain(
      "Plain text version of the update.",
    );
    expect(wrapper.find("iframe").exists()).toBe(false);
  });

  it("defaults to text and hides the HTML tab when there is no html part", async () => {
    getMessageDetailMock.mockResolvedValue({
      ...htmlDetail,
      sanitizedHtml: null,
      remoteBlocked: 0,
      hadActiveContent: false,
    });
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const tabLabels = wrapper.findAll("button.tab").map((tab) => tab.text());
    expect(tabLabels).toContain("Text");
    expect(tabLabels).not.toContain("HTML");
    expect(wrapper.get("pre.text").text()).toContain("Plain text version");
  });

  it("shows an unavailable state when the detail cannot be loaded", async () => {
    getMessageDetailMock.mockResolvedValue(null);
    const wrapper = mount(MessageViewPane, { props: { messageId: 777 } });
    await flushPromises();

    expect(wrapper.text()).toContain("Message unavailable");
  });
});
