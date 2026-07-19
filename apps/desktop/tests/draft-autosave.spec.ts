import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  useDraftAutosave,
  type DraftAutosaveOptions,
} from "../src/composables/useDraftAutosave";

/** A render-less host so the composable's lifecycle hooks run. */
function host(options: DraftAutosaveOptions) {
  return defineComponent({
    setup() {
      useDraftAutosave(options);
      return () => null;
    },
  });
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useDraftAutosave", () => {
  it("saves on the interval once the content changes", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const save = vi.fn().mockResolvedValue(undefined);
    let content = "start";
    mount(
      host({
        intervalMs: 1000,
        snapshot: () => content,
        isEmpty: () => content.length === 0,
        isBusy: () => false,
        isFinished: () => false,
        save,
      }),
    );

    // Unchanged from the baseline snapshot: nothing is written.
    vi.advanceTimersByTime(1000);
    await flushPromises();
    expect(save).not.toHaveBeenCalled();

    // After an edit, the next tick persists it exactly once.
    content = "start typing";
    vi.advanceTimersByTime(1000);
    await flushPromises();
    expect(save).toHaveBeenCalledTimes(1);

    // A further unchanged tick does not write again.
    vi.advanceTimersByTime(1000);
    await flushPromises();
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("never saves an empty composer", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const save = vi.fn().mockResolvedValue(undefined);
    mount(
      host({
        intervalMs: 500,
        snapshot: () => "",
        isEmpty: () => true,
        isBusy: () => false,
        isFinished: () => false,
        save,
      }),
    );
    vi.advanceTimersByTime(2000);
    await flushPromises();
    expect(save).not.toHaveBeenCalled();
  });

  it("defers while a manual send or save is in flight", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const save = vi.fn().mockResolvedValue(undefined);
    let content = "draft";
    mount(
      host({
        intervalMs: 500,
        snapshot: () => content,
        isEmpty: () => false,
        isBusy: () => true,
        isFinished: () => false,
        save,
      }),
    );
    content = "changed while busy";
    vi.advanceTimersByTime(500);
    await flushPromises();
    expect(save).not.toHaveBeenCalled();
  });

  it("flushes on close when the content changed", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    let content = "baseline";
    const wrapper = mount(
      host({
        snapshot: () => content,
        isEmpty: () => false,
        isBusy: () => false,
        isFinished: () => false,
        save,
      }),
    );
    content = "edited before close";
    wrapper.unmount();
    await flushPromises();
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("does not resurrect a sent message on close", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    let content = "baseline";
    const wrapper = mount(
      host({
        snapshot: () => content,
        isEmpty: () => false,
        isBusy: () => false,
        isFinished: () => true,
        save,
      }),
    );
    content = "edited";
    wrapper.unmount();
    await flushPromises();
    expect(save).not.toHaveBeenCalled();
  });
});
