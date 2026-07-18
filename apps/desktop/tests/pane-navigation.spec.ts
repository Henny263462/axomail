import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";

import AppShell from "../src/components/layout/AppShell.vue";

type ChangeListener = (event: { matches: boolean }) => void;

function stubMatchMedia(matches: boolean) {
  const listeners: ChangeListener[] = [];
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addEventListener: (_type: string, listener: ChangeListener) => {
      listeners.push(listener);
    },
    removeEventListener: (_type: string, listener: ChangeListener) => {
      const index = listeners.indexOf(listener);
      if (index >= 0) listeners.splice(index, 1);
    },
  }));
  return {
    dispatchChange(next: boolean) {
      for (const listener of [...listeners]) listener({ matches: next });
    },
    listenerCount: () => listeners.length,
  };
}

const slots = {
  sidebar: "<div data-test='sidebar'>S</div>",
  list: "<div data-test='list'>L</div>",
  message: "<div data-test='message'>M</div>",
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("AppShell", () => {
  it("shows all three panes side by side on wide viewports", () => {
    stubMatchMedia(false);
    const wrapper = mount(AppShell, { slots });

    expect(wrapper.find('[aria-label="Panes"]').exists()).toBe(false);
    for (const pane of ["sidebar", "list", "message"]) {
      expect(wrapper.get(`[data-test="${pane}"]`).isVisible()).toBe(true);
    }
  });

  it("stacks panes and switches via the pane navigation when narrow", async () => {
    stubMatchMedia(true);
    const wrapper = mount(AppShell, { slots });

    const hidden = (selector: string) =>
      (wrapper.get(selector).attributes("style") ?? "").includes(
        "display: none",
      );

    expect(wrapper.get('[aria-label="Panes"]').isVisible()).toBe(true);
    expect(hidden(".col-list")).toBe(false);
    expect(hidden(".col-message")).toBe(true);
    expect(hidden(".col-sidebar")).toBe(true);

    const tabs = wrapper.findAll("button.stack-tab");
    const readingTab = tabs.find((tab) => tab.text() === "Reading");
    if (readingTab === undefined) throw new Error("Reading tab missing");
    await readingTab.trigger("click");

    expect(hidden(".col-message")).toBe(false);
    expect(hidden(".col-list")).toBe(true);
  });

  it("reacts to viewport changes via the media-query listener", async () => {
    const media = stubMatchMedia(false);
    const wrapper = mount(AppShell, { slots });

    expect(wrapper.find('[aria-label="Panes"]').exists()).toBe(false);

    media.dispatchChange(true);
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[aria-label="Panes"]').exists()).toBe(true);

    media.dispatchChange(false);
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[aria-label="Panes"]').exists()).toBe(false);
  });

  it("removes the media-query listener on unmount", () => {
    const media = stubMatchMedia(false);
    const wrapper = mount(AppShell, { slots });
    expect(media.listenerCount()).toBe(1);
    wrapper.unmount();
    expect(media.listenerCount()).toBe(0);
  });
});
