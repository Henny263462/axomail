import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getMessageDetail } from "../src/api/messages";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import { provideCommandRegistry } from "../src/composables/useCommandRegistry";
import { printMessageDocument } from "../src/lib/printDocument";
import type { MessageDetailDto } from "../src/types/mail";

vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
  setMessageFlagged: vi.fn(),
}));
vi.mock("../src/api/invitations", () => ({ respondInvitation: vi.fn() }));
vi.mock("../src/lib/printDocument", () => ({ printMessageDocument: vi.fn() }));

const getMessageDetailMock = vi.mocked(getMessageDetail);
const printMock = vi.mocked(printMessageDocument);

function detail(): MessageDetailDto {
  return {
    id: 501,
    subject: "Print me",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    to: [{ name: "Klaus", address: "klaus@theferbers.eu" }],
    cc: [],
    dateUnix: 1_752_790_000,
    flagged: false,
    textBody: "Body text.",
    sanitizedHtml: null,
    remoteBlockedCount: 0,
    remoteAllowed: false,
    scamFlags: [],
    hadActiveContent: false,
    attachments: [],
    messageId: "m-1@example.org",
    inReplyTo: [],
    references: [],
    invitation: null,
  };
}

/** Dispatches a real global keydown (the registry listener is on window). */
function press(init: KeyboardEventInit): void {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
  );
}

/** Mounts the pane under a host that installs the app-root command registry. */
function host(props: { messageId: number | null; isModalOpen: () => boolean }) {
  const Root = defineComponent({
    setup() {
      provideCommandRegistry();
      return () => h(MessageViewPane, props);
    },
  });
  return mount(Root, { attachTo: document.body });
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("MessageViewPane print shortcut gating", () => {
  it("prints the open message on Mod+P when no modal is open", async () => {
    getMessageDetailMock.mockResolvedValue(detail());
    const wrapper = host({ messageId: 501, isModalOpen: () => false });
    await flushPromises();

    press({ key: "p", ctrlKey: true });
    expect(printMock).toHaveBeenCalledOnce();
    wrapper.unmount();
  });

  it("does not print on Mod+P while a modal covers the message", async () => {
    getMessageDetailMock.mockResolvedValue(detail());
    const wrapper = host({ messageId: 501, isModalOpen: () => true });
    await flushPromises();

    press({ key: "p", ctrlKey: true });
    expect(printMock).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
