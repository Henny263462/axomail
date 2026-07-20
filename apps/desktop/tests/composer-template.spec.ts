import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { saveDraft, sendMessage } from "../src/api/compose";
import { listIdentities } from "../src/api/identities";
import { saveTemplate } from "../src/api/templates";
import ComposerDialog from "../src/components/compose/ComposerDialog.vue";

vi.mock("../src/api/compose", () => ({
  sendMessage: vi.fn(),
  saveDraft: vi.fn(),
}));
vi.mock("../src/api/identities", () => ({ listIdentities: vi.fn() }));
vi.mock("../src/api/reply", () => ({ markReplySource: vi.fn() }));
vi.mock("../src/api/templates", () => ({ saveTemplate: vi.fn() }));
vi.mock("../src/api/attachments", () => ({
  pickAttachments: vi.fn(),
  statAttachments: vi.fn(),
  onFilesDropped: vi.fn(() => () => {}),
}));

const listIdentitiesMock = vi.mocked(listIdentities);
const saveTemplateMock = vi.mocked(saveTemplate);

beforeEach(() => {
  vi.clearAllMocks();
  listIdentitiesMock.mockResolvedValue([]);
  saveTemplateMock.mockResolvedValue({
    id: 1,
    name: "Weekly note",
    subject: "Weekly note",
    to: [],
    cc: [],
    bcc: [],
    bodyText: "",
  });
  vi.mocked(sendMessage).mockResolvedValue(null);
  vi.mocked(saveDraft).mockResolvedValue(null);
});

function templateButton(wrapper: VueWrapper) {
  const button = wrapper
    .findAll("button")
    .find((candidate) => candidate.text() === "Save as template");
  if (button === undefined) throw new Error("save-as-template button missing");
  return button;
}

describe("composer templates", () => {
  it("pre-fills a fresh composition from an initial (template) seed", async () => {
    const wrapper = mount(ComposerDialog, {
      props: {
        accountId: 42,
        initial: {
          to: ["team@example.org"],
          subject: "Weekly note",
          body: "Hello team,",
        },
      },
    });
    await flushPromises();

    expect((wrapper.get("#compose-subject").element as HTMLInputElement).value).toBe(
      "Weekly note",
    );
    expect((wrapper.get("#compose-to").element as HTMLInputElement).value).toBe(
      "team@example.org",
    );
  });

  it("saves the current content as a template named after the subject", async () => {
    const wrapper = mount(ComposerDialog, {
      props: {
        accountId: 42,
        initial: {
          to: ["team@example.org"],
          subject: "Weekly note",
          body: "Hello team,",
        },
      },
    });
    await flushPromises();

    await templateButton(wrapper).trigger("click");
    await flushPromises();

    expect(saveTemplateMock).toHaveBeenCalledTimes(1);
    expect(saveTemplateMock).toHaveBeenCalledWith({
      name: "Weekly note",
      subject: "Weekly note",
      to: ["team@example.org"],
      cc: [],
      bcc: [],
      bodyText: "Hello team,",
    });
  });
});
