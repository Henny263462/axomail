import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";

import { subscribePushStatus } from "../src/api/push";
import { getPushEnabled, setPushEnabled } from "../src/api/settings";
import type { SettingsSectionContext } from "../src/components/settings/context";
import PushSection from "../src/components/settings/PushSection.vue";
import { usePushStatus } from "../src/composables/usePushStatus";
import type { PushStatusDto } from "../src/types/mail";

vi.mock("../src/api/push", () => ({
  subscribePushStatus: vi.fn(),
}));

vi.mock("../src/api/settings", () => ({
  getPushEnabled: vi.fn(),
  setPushEnabled: vi.fn(),
}));

const subscribePushStatusMock = vi.mocked(subscribePushStatus);
const getPushEnabledMock = vi.mocked(getPushEnabled);
const setPushEnabledMock = vi.mocked(setPushEnabled);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

let pushHandler: ((event: PushStatusDto) => void) | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  pushHandler = null;
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  subscribePushStatusMock.mockImplementation((handler) => {
    pushHandler = handler;
    return Promise.resolve(() => {});
  });
  getPushEnabledMock.mockResolvedValue(true);
  setPushEnabledMock.mockResolvedValue(null);
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

/** A harness that renders the composable's outputs for assertions. */
const Harness = defineComponent({
  setup() {
    const { pushStateFor, anyConnected } = usePushStatus();
    return () =>
      h("div", [
        h("span", { class: "state-1" }, pushStateFor(1)),
        h("span", { class: "any" }, String(anyConnected.value)),
      ]);
  },
});

describe("usePushStatus", () => {
  it("defaults an unheard-of account to polling", async () => {
    const wrapper = mount(Harness);
    await flushPromises();
    expect(wrapper.get(".state-1").text()).toBe("polling");
    expect(wrapper.get(".any").text()).toBe("false");
  });

  it("tracks connected then reconnecting per account", async () => {
    const wrapper = mount(Harness);
    await flushPromises();
    if (pushHandler === null) throw new Error("push handler not registered");

    pushHandler({ accountId: 1, mailboxId: 10, state: "connected" });
    await flushPromises();
    expect(wrapper.get(".state-1").text()).toBe("connected");
    expect(wrapper.get(".any").text()).toBe("true");

    pushHandler({ accountId: 1, mailboxId: null, state: "reconnecting" });
    await flushPromises();
    expect(wrapper.get(".state-1").text()).toBe("reconnecting");
    expect(wrapper.get(".any").text()).toBe("false");
  });
});

/** A minimal context: PushSection only reports status/error. */
function stubContext(): SettingsSectionContext {
  return {
    accounts: [],
    initialPolicies: [],
    notificationsEnabled: false,
    syncIntervalMinutes: 5,
    theme: "system",
    autoMarkReadEnabled: true,
    autoMarkReadDelayMs: 0,
    setNotificationsEnabled: vi.fn(),
    setSyncIntervalMinutes: vi.fn(),
    setTheme: vi.fn(),
    setAutoMarkRead: vi.fn(),
    reportError: vi.fn(),
    reportStatus: vi.fn(),
    notifyAccountsChanged: vi.fn(),
    notifyTagsChanged: vi.fn(),
  };
}

describe("PushSection", () => {
  it("reflects the loaded push setting with a labelled toggle", async () => {
    getPushEnabledMock.mockResolvedValue(false);
    const wrapper = mount(PushSection, { props: { context: stubContext() } });
    await flushPromises();

    expect(wrapper.get("legend").text()).toContain("Push (IMAP IDLE)");
    const box = wrapper.get('input[type="checkbox"]').element as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(
      wrapper.find('input[aria-label="Use push (IMAP IDLE) when available"]')
        .exists(),
    ).toBe(true);
  });

  it("persists a toggle and announces the outcome", async () => {
    const context = stubContext();
    const wrapper = mount(PushSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[type="checkbox"]').setValue(false);
    await flushPromises();

    expect(setPushEnabledMock).toHaveBeenCalledWith(false);
    expect(context.reportStatus).toHaveBeenLastCalledWith(
      expect.stringContaining("poll"),
    );
  });

  it("refuses the toggle in the browser preview", async () => {
    delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
    const context = stubContext();
    const wrapper = mount(PushSection, { props: { context } });
    await flushPromises();

    await wrapper.get('input[type="checkbox"]').setValue(false);

    expect(setPushEnabledMock).not.toHaveBeenCalled();
    expect(context.reportError).toHaveBeenCalledWith(
      expect.stringContaining("browser preview"),
    );
  });
});
