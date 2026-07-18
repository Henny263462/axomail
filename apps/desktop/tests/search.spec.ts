import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { searchMessages } from "../src/api/search";
import MailListRegion from "../src/components/list/MailListRegion.vue";
import type { SearchResultDto } from "../src/types/mail";

vi.mock("../src/api/search", () => ({ searchMessages: vi.fn() }));

const searchMessagesMock = vi.mocked(searchMessages);

const results: SearchResultDto[] = [
  {
    messageId: 101,
    mailboxId: 10,
    accountId: 1,
    subject: "Design tokens",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    dateUnix: 1_752_000_000,
    // Contains an FTS match marker and raw HTML that must never be injected.
    snippet: "the [design] tokens <b>x</b>",
  },
];

function mountRegion() {
  return mount(MailListRegion, {
    props: {
      view: "mailbox",
      messages: [],
      unifiedItems: [],
      accounts: [],
      selectedMessageId: null,
    selectedMailboxId: null,
    },
  });
}

function searchInput(wrapper: ReturnType<typeof mountRegion>) {
  return wrapper.get('input[role="searchbox"]');
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  searchMessagesMock.mockResolvedValue(results);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("MailListRegion search", () => {
  it("debounces rapid typing into a single request", async () => {
    const wrapper = mountRegion();
    const input = searchInput(wrapper);

    await input.setValue("d");
    await vi.advanceTimersByTimeAsync(100);
    await input.setValue("de");
    await vi.advanceTimersByTimeAsync(100);
    await input.setValue("des");
    expect(searchMessagesMock).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(250);
    expect(searchMessagesMock).toHaveBeenCalledTimes(1);
    expect(searchMessagesMock).toHaveBeenCalledWith("des", 30);
  });

  it("enters results mode and renders the snippet as plain text", async () => {
    const wrapper = mountRegion();

    await searchInput(wrapper).setValue("design");
    await vi.advanceTimersByTimeAsync(250);

    expect(wrapper.find('[aria-label="Search results"]').exists()).toBe(true);
    expect(wrapper.find('[aria-label="Message list"]').exists()).toBe(false);
    expect(wrapper.get('[role="status"]').text()).toContain("1 result");
    expect(wrapper.text()).toContain("Design tokens");

    // The FTS match marker is styled, not left as literal brackets.
    expect(wrapper.get("mark").text()).toBe("design");
    // Raw HTML in the snippet is escaped: no real element is created.
    expect(wrapper.find("b").exists()).toBe(false);
    expect(wrapper.text()).toContain("<b>x</b>");
  });

  it("restores the mailbox list on Escape", async () => {
    const wrapper = mountRegion();

    await searchInput(wrapper).setValue("design");
    await vi.advanceTimersByTimeAsync(250);
    expect(wrapper.find('[aria-label="Search results"]').exists()).toBe(true);

    await searchInput(wrapper).trigger("keydown", { key: "Escape" });

    expect(wrapper.find('[aria-label="Search results"]').exists()).toBe(false);
    expect(wrapper.find('[aria-label="Message list"]').exists()).toBe(true);
    expect((searchInput(wrapper).element as HTMLInputElement).value).toBe("");
  });

  it("emits the message id when a result is chosen", async () => {
    const wrapper = mountRegion();

    await searchInput(wrapper).setValue("design");
    await vi.advanceTimersByTimeAsync(250);

    await wrapper.get("button.item").trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual([101]);
  });
});
