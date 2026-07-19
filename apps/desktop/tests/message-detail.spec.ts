import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  getMessageDetail,
  setRemoteContentAllowed,
} from "../src/api/messages";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import type { MessageDetailDto } from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
  setMessageFlagged: vi.fn(),
}));

const getMessageDetailMock = vi.mocked(getMessageDetail);
const setRemoteContentAllowedMock = vi.mocked(setRemoteContentAllowed);

const htmlDetail: MessageDetailDto = {
  id: 501,
  subject: "Quarterly update",
  fromName: "Ada Lovelace",
  fromAddress: "ada@example.org",
  to: [{ name: "Klaus", address: "klaus@theferbers.eu" }],
  cc: [{ name: null, address: "cc@example.org" }],
  dateUnix: 1_752_790_000,
  flagged: false,
  textBody: "Plain text version of the update.",
  sanitizedHtml: '<p>Hello <a href="https://example.org">link</a></p>',
  remoteBlockedCount: 3,
  remoteAllowed: false,
  scamFlags: [],
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
  setRemoteContentAllowedMock.mockResolvedValue(null);
});

describe("MessageViewPane detail", () => {
  it("shows a loading state then renders the header and notices", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });

    expect(wrapper.text()).toContain("Loading message…");

    await flushPromises();

    expect(getMessageDetailMock).toHaveBeenCalledWith(501, null);
    expect(wrapper.text()).toContain("Quarterly update");
    expect(wrapper.text()).toContain("klaus@theferbers.eu");
    expect(wrapper.text()).toContain("Remote content blocked (3)");
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
      .find((tab) => tab.text() === "Plain Text");
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
      remoteBlockedCount: 0,
      hadActiveContent: false,
    });
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const tabLabels = wrapper.findAll("button.tab").map((tab) => tab.text());
    expect(tabLabels).toContain("Plain Text");
    expect(tabLabels).not.toContain("Sanitized HTML");
    expect(wrapper.get("pre.text").text()).toContain("Plain text version");
  });

  it("shows an unavailable state when the detail cannot be loaded", async () => {
    getMessageDetailMock.mockResolvedValue(null);
    const wrapper = mount(MessageViewPane, { props: { messageId: 777 } });
    await flushPromises();

    expect(wrapper.text()).toContain("Message unavailable");
  });
});

describe("MessageViewPane remote content", () => {
  it("re-fetches with the allow-once override when Show once is clicked", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const infoBar = wrapper.get('[role="status"]');
    expect(infoBar.text()).toContain("Remote content blocked (3)");

    const showOnce = wrapper
      .findAll("button.bar-action")
      .find((button) => button.text() === "Show once");
    if (showOnce === undefined) throw new Error("Show once button missing");
    await showOnce.trigger("click");
    await flushPromises();

    // Second load carries the allow-once override; nothing is persisted.
    expect(getMessageDetailMock).toHaveBeenLastCalledWith(501, true);
    expect(setRemoteContentAllowedMock).not.toHaveBeenCalled();
  });

  it("persists the sender then re-fetches when Always allow is clicked", async () => {
    getMessageDetailMock.mockResolvedValue(htmlDetail);
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const alwaysAllow = wrapper
      .findAll("button.bar-action")
      .find((button) => button.text().startsWith("Always allow"));
    if (alwaysAllow === undefined) throw new Error("Always allow button missing");
    expect(alwaysAllow.text()).toContain("ada@example.org");
    await alwaysAllow.trigger("click");
    await flushPromises();

    expect(setRemoteContentAllowedMock).toHaveBeenCalledWith(
      "ada@example.org",
      true,
    );
    // The re-fetch defers to the stored decision (no override).
    expect(getMessageDetailMock).toHaveBeenLastCalledWith(501, null);
  });

  it("hides the remote-content bar once images are allowed", async () => {
    getMessageDetailMock.mockResolvedValue({
      ...htmlDetail,
      remoteBlockedCount: 0,
      remoteAllowed: true,
    });
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    expect(wrapper.find('[role="status"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Remote content blocked");
  });

  it("renders a scam warning as an alert listing every reason", async () => {
    getMessageDetailMock.mockResolvedValue({
      ...htmlDetail,
      scamFlags: [
        { kind: "deceptiveLink", detail: 'Link text shows "paypal.com" but the link goes to "evil.example".' },
        { kind: "ipAddressHost", detail: "This link points to a numeric IP address (192.168.0.1)." },
      ],
    });
    const wrapper = mount(MessageViewPane, { props: { messageId: 501 } });
    await flushPromises();

    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain("possible scam");
    expect(alert.text()).toContain("paypal.com");
    expect(alert.text()).toContain("192.168.0.1");
    // Non-dismissive: there is no control to close the warning.
    expect(alert.find("button").exists()).toBe(false);
  });
});
