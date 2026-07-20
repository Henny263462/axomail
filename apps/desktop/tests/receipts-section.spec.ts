import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getReceiptSettings, setReceiptSettings } from "../src/api/receipts";
import ReceiptsSection from "../src/components/settings/ReceiptsSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import { isTauri } from "../src/lib/tauri";
import type { ReceiptSettings } from "../src/types/receipts";

vi.mock("../src/api/receipts", () => ({
  getReceiptSettings: vi.fn(),
  setReceiptSettings: vi.fn(),
}));
vi.mock("../src/lib/tauri", () => ({ isTauri: vi.fn() }));

const getMock = vi.mocked(getReceiptSettings);
const setMock = vi.mocked(setReceiptSettings);
const isTauriMock = vi.mocked(isTauri);

function context(): SettingsSectionContext {
  return {
    reportStatus: vi.fn(),
    reportError: vi.fn(),
  } as unknown as SettingsSectionContext;
}

const defaults: ReceiptSettings = {
  policy: "ask",
  onlyWhenAddressed: true,
  neverForMailingLists: true,
};

/** A promise the test resolves/rejects later, to order overlapping saves. */
function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.clearAllMocks();
  getMock.mockResolvedValue({ ...defaults });
  setMock.mockResolvedValue(null);
  isTauriMock.mockReturnValue(true);
});

describe("ReceiptsSection", () => {
  it("loads the stored settings and reflects the policy", async () => {
    getMock.mockResolvedValueOnce({
      policy: "always",
      onlyWhenAddressed: false,
      neverForMailingLists: false,
    });
    const wrapper = mount(ReceiptsSection, { props: { context: context() } });
    await flushPromises();
    const always = wrapper.get('input[value="always"]');
    expect((always.element as HTMLInputElement).checked).toBe(true);
  });

  it("persists the whole settings object when the policy changes", async () => {
    const ctx = context();
    const wrapper = mount(ReceiptsSection, { props: { context: ctx } });
    await flushPromises();

    await wrapper.get('input[value="never"]').setValue();
    await flushPromises();

    expect(setMock).toHaveBeenCalledWith({
      policy: "never",
      onlyWhenAddressed: true,
      neverForMailingLists: true,
    });
    expect(ctx.reportStatus).toHaveBeenCalledWith("Read-receipt settings updated.");
  });

  it("persists a condition toggle", async () => {
    const wrapper = mount(ReceiptsSection, { props: { context: context() } });
    await flushPromises();

    // The first checkbox is "only when addressed to me"; untick it.
    const onlyWhenAddressed = wrapper.findAll('input[type="checkbox"]').at(0);
    await onlyWhenAddressed?.setValue(false);
    await flushPromises();

    expect(setMock).toHaveBeenCalledWith({
      policy: "ask",
      onlyWhenAddressed: false,
      neverForMailingLists: true,
    });
  });

  it("reverts and reports an error when a save fails", async () => {
    setMock.mockRejectedValueOnce(new Error("nope"));
    const ctx = context();
    const wrapper = mount(ReceiptsSection, { props: { context: ctx } });
    await flushPromises();

    await wrapper.get('input[value="always"]').setValue();
    await flushPromises();

    expect(ctx.reportError).toHaveBeenCalled();
    // The selection reverted to the loaded "ask".
    expect((wrapper.get('input[value="ask"]').element as HTMLInputElement).checked).toBe(
      true,
    );
  });

  // Finding 9: an earlier failed save must not revert a newer applied value.
  it("does not let a stale save failure clobber a newer successful change", async () => {
    const saveA = deferred<null>();
    const saveB = deferred<null>();
    setMock.mockReturnValueOnce(saveA.promise).mockReturnValueOnce(saveB.promise);
    const ctx = context();
    const wrapper = mount(ReceiptsSection, { props: { context: ctx } });
    await flushPromises();

    // Two rapid, overlapping changes: A ("always") then B ("never").
    await wrapper.get('input[value="always"]').setValue();
    await wrapper.get('input[value="never"]').setValue();

    // The newer change (B) succeeds; the earlier one (A) fails afterwards.
    saveB.resolve(null);
    saveA.reject(new Error("stale failure"));
    await flushPromises();

    // B's applied value survives; A's late failure does not revert to "ask".
    expect(
      (wrapper.get('input[value="never"]').element as HTMLInputElement).checked,
    ).toBe(true);
  });
});
