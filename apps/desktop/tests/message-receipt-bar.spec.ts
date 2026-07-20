import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sendMdn } from "../src/api/receipts";
import MessageReceiptBar from "../src/components/message/MessageReceiptBar.vue";
import type { ReceiptRequestDto } from "../src/types/receipts";

vi.mock("../src/api/receipts", () => ({ sendMdn: vi.fn() }));

const sendMdnMock = vi.mocked(sendMdn);

function mountBar(request: ReceiptRequestDto) {
  return mount(MessageReceiptBar, {
    props: { receipt: request, messageId: 42 },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  sendMdnMock.mockResolvedValue({ sent: true, status: "sent", error: null });
});

describe("MessageReceiptBar", () => {
  it("prompts for an ask request and sends on click", async () => {
    const wrapper = mountBar({ to: "ada@example.org", action: "ask" });
    // The prompt is shown; no receipt sent yet.
    expect(wrapper.text()).toContain("The sender requested a read receipt.");
    expect(sendMdnMock).not.toHaveBeenCalled();

    await wrapper.find(".bar-action").trigger("click");
    await flushPromises();

    expect(sendMdnMock).toHaveBeenCalledWith(42);
    expect(wrapper.text()).toContain("Read receipt sent.");
  });

  it("dismisses without sending when the user ignores", async () => {
    const wrapper = mountBar({ to: "ada@example.org", action: "ask" });
    const ignore = wrapper.findAll(".bar-action").at(1);
    await ignore?.trigger("click");
    expect(sendMdnMock).not.toHaveBeenCalled();
    expect(wrapper.find(".receipt-bar").exists()).toBe(false);
  });

  it("auto-sends for an always request on mount", async () => {
    const wrapper = mountBar({ to: "ada@example.org", action: "send" });
    await flushPromises();
    expect(sendMdnMock).toHaveBeenCalledWith(42);
    expect(wrapper.text()).toContain("Read receipt sent.");
  });

  it("surfaces a retry affordance when sending fails", async () => {
    sendMdnMock.mockResolvedValueOnce(null);
    const wrapper = mountBar({ to: "ada@example.org", action: "send" });
    await flushPromises();
    expect(wrapper.text()).toContain("Could not send the read receipt.");
    // A retry control is offered.
    expect(wrapper.find(".bar-action").exists()).toBe(true);
  });
});
