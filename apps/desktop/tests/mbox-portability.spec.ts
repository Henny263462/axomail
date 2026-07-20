/* eslint-disable vue/one-component-per-file --
   Tiny throwaway host components give the composable a component scope and
   provide the command registry; the one-component-per-file rule is for SFCs. */
import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getMessageDetail } from "../src/api/messages";
import {
  exportFolderMbox,
  importMbox,
  pickMboxImportPath,
  pickMboxSavePath,
} from "../src/api/mbox";
import FolderContextMenu from "../src/components/sidebar/FolderContextMenu.vue";
import MessageActions from "../src/components/message/MessageActions.vue";
import MessageViewPane from "../src/components/message/MessageViewPane.vue";
import { provideCommandRegistry } from "../src/composables/useCommandRegistry";
import { useFolderImportExport } from "../src/composables/useFolderImportExport";
import { printMessageDocument } from "../src/lib/printDocument";
import type { MailboxSummary, MessageDetailDto } from "../src/types/mail";

vi.mock("../src/api/mbox", () => ({
  exportFolderMbox: vi.fn(),
  importMbox: vi.fn(),
  pickMboxSavePath: vi.fn(),
  pickMboxImportPath: vi.fn(),
}));

vi.mock("../src/api/messages", () => ({
  getMessageDetail: vi.fn(),
  setMessageFlagged: vi.fn(),
  setRemoteContentAllowed: vi.fn(),
}));

vi.mock("../src/api/attachments", () => ({
  getMessageHeaders: vi.fn(),
  getMessageSource: vi.fn(),
  pickSaveFilePath: vi.fn(),
  saveMessageEml: vi.fn(),
}));

vi.mock("../src/lib/printDocument", () => ({
  printMessageDocument: vi.fn(),
  buildPrintDocument: vi.fn(() => "<html></html>"),
}));

const exportMock = vi.mocked(exportFolderMbox);
const importMock = vi.mocked(importMbox);
const pickSaveMock = vi.mocked(pickMboxSavePath);
const pickOpenMock = vi.mocked(pickMboxImportPath);
const getDetailMock = vi.mocked(getMessageDetail);
const printMock = vi.mocked(printMessageDocument);

function folder(over: Partial<MailboxSummary> = {}): MailboxSummary {
  return { id: 3, name: "Archive", role: "archive", unreadCount: 0, ...over };
}

function detail(over: Partial<MessageDetailDto> = {}): MessageDetailDto {
  return {
    id: 5,
    subject: "Subject",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    to: [],
    cc: [],
    dateUnix: 1_752_000_000,
    flagged: false,
    textBody: "body",
    sanitizedHtml: null,
    remoteBlockedCount: 0,
    remoteAllowed: false,
    scamFlags: [],
    hadActiveContent: false,
    attachments: [],
    messageId: null,
    inReplyTo: [],
    references: [],
    ...over,
  };
}

/** Mounts a throwaway host so the composable runs in a component scope. */
function useHost(onChanged: () => void) {
  let api!: ReturnType<typeof useFolderImportExport>;
  const wrapper = mount(
    defineComponent({
      setup() {
        api = useFolderImportExport(onChanged);
        return () => null;
      },
    }),
  );
  return { api, wrapper };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useFolderImportExport", () => {
  it("exports to the chosen path and reports the counts", async () => {
    pickSaveMock.mockResolvedValue("/tmp/Archive.mbox");
    exportMock.mockResolvedValue({ exported: 3, skipped: 1 });
    const { api, wrapper } = useHost(() => {});

    await api.exportFolder(folder());
    expect(pickSaveMock).toHaveBeenCalledWith("Archive.mbox");
    expect(exportMock).toHaveBeenCalledWith(3, "/tmp/Archive.mbox");
    expect(api.message.value).toBe("Exported 3 message(s), 1 skipped.");
    wrapper.unmount();
  });

  it("does nothing when the save dialog is cancelled", async () => {
    pickSaveMock.mockResolvedValue(null);
    const { api, wrapper } = useHost(() => {});

    await api.exportFolder(folder());
    expect(exportMock).not.toHaveBeenCalled();
    expect(api.message.value).toBeNull();
    wrapper.unmount();
  });

  it("imports the chosen file and notifies the caller to reload", async () => {
    pickOpenMock.mockResolvedValue("/tmp/in.mbox");
    importMock.mockResolvedValue({ imported: 2 });
    const onChanged = vi.fn();
    const { api, wrapper } = useHost(onChanged);

    await api.importFolder(folder());
    expect(importMock).toHaveBeenCalledWith(3, "/tmp/in.mbox");
    expect(onChanged).toHaveBeenCalledOnce();
    expect(api.message.value).toBe("Imported 2 message(s) into Archive.");
    wrapper.unmount();
  });

  it("surfaces a backend error message on import failure", async () => {
    pickOpenMock.mockResolvedValue("/tmp/in.mbox");
    importMock.mockRejectedValue(new Error("the target folder does not exist"));
    const onChanged = vi.fn();
    const { api, wrapper } = useHost(onChanged);

    await api.importFolder(folder());
    expect(onChanged).not.toHaveBeenCalled();
    expect(api.message.value).toBe("the target folder does not exist");
    wrapper.unmount();
  });
});

describe("FolderContextMenu portability items", () => {
  it("emits exportFolder and importFolder from the menu", async () => {
    const wrapper = mount(FolderContextMenu, {
      props: { folder: folder(), x: 0, y: 0 },
    });
    const labels = wrapper.findAll('[role="menuitem"]');
    const exportItem = labels.find((b) => b.text() === "Export folder…");
    const importItem = labels.find((b) => b.text() === "Import messages…");
    expect(exportItem).toBeDefined();
    expect(importItem).toBeDefined();

    await exportItem?.trigger("click");
    await importItem?.trigger("click");
    expect(wrapper.emitted("exportFolder")).toHaveLength(1);
    expect(wrapper.emitted("importFolder")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("MessageActions print button", () => {
  it("emits print when the Print action is clicked", async () => {
    const wrapper = mount(MessageActions, {
      props: { messageId: 5, subject: "Hi" },
    });
    const printButton = wrapper.findAll("button").find((b) => b.text() === "Print");
    expect(printButton).toBeDefined();
    await printButton?.trigger("click");
    expect(wrapper.emitted("print")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("MessageViewPane print wiring", () => {
  it("prints the open message when the Print button is used", async () => {
    getDetailMock.mockResolvedValue(detail({ id: 5 }));
    const wrapper = mount(MessageViewPane, { props: { messageId: 5 } });
    await flushPromises();

    const printButton = wrapper.findAll("button").find((b) => b.text() === "Print");
    await printButton?.trigger("click");
    expect(printMock).toHaveBeenCalledOnce();
    expect(printMock.mock.calls[0]?.[0]?.id).toBe(5);
    wrapper.unmount();
  });

  it("registers a global print command that fires on the accelerator", async () => {
    getDetailMock.mockResolvedValue(detail({ id: 9 }));
    // A host that provides the command registry, as App.vue does at the root.
    const host = mount(
      defineComponent({
        setup() {
          provideCommandRegistry();
          return () => h(MessageViewPane, { messageId: 9 });
        },
      }),
    );
    await flushPromises();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", ctrlKey: true }));
    expect(printMock).toHaveBeenCalledOnce();
    expect(printMock.mock.calls[0]?.[0]?.id).toBe(9);
    host.unmount();
  });

  it("does not print via the accelerator when no message is open", async () => {
    const host = mount(
      defineComponent({
        setup() {
          provideCommandRegistry();
          return () => h(MessageViewPane, { messageId: null });
        },
      }),
    );
    await flushPromises();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", ctrlKey: true }));
    expect(printMock).not.toHaveBeenCalled();
    host.unmount();
  });
});
