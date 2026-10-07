import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import LayoutSwitcher from "../src/components/layout/LayoutSwitcher.vue";
import SidebarViewSwitcher from "../src/components/sidebar/SidebarViewSwitcher.vue";
import { setLayout } from "../src/composables/useLayout";

afterEach(() => {
  document.body.innerHTML = "";
});

// Finding 12: the radiogroup switchers implement roving arrow-key selection.
describe("LayoutSwitcher arrow-key navigation", () => {
  beforeEach(() => setLayout("vertical")); // index 1 of [classic, vertical, wide]

  it("moves and selects the adjacent layout with arrow keys", async () => {
    const wrapper = mount(LayoutSwitcher, { attachTo: document.body });
    const radios = wrapper.findAll('[role="radio"]');
    // Roving tabindex: only the checked radio is tabbable.
    expect(radios[1]!.attributes("aria-checked")).toBe("true");
    expect(radios[1]!.attributes("tabindex")).toBe("0");
    expect(radios[0]!.attributes("tabindex")).toBe("-1");

    await radios[1]!.trigger("keydown", { key: "ArrowRight" });
    // Right → "wide" (index 2), selection follows focus.
    expect(
      wrapper.findAll('[role="radio"]')[2]!.attributes("aria-checked"),
    ).toBe("true");

    await wrapper
      .findAll('[role="radio"]')[2]!
      .trigger("keydown", { key: "ArrowLeft" });
    expect(
      wrapper.findAll('[role="radio"]')[1]!.attributes("aria-checked"),
    ).toBe("true");
    wrapper.unmount();
  });

  it("wraps around at the ends", async () => {
    setLayout("classic"); // index 0
    const wrapper = mount(LayoutSwitcher, { attachTo: document.body });
    await wrapper
      .findAll('[role="radio"]')[0]!
      .trigger("keydown", { key: "ArrowLeft" });
    expect(
      wrapper.findAll('[role="radio"]')[3]!.attributes("aria-checked"),
    ).toBe("true");
    wrapper.unmount();
  });
});

describe("SidebarViewSwitcher arrow-key navigation", () => {
  it("emits select for the adjacent mode on arrow keys, wrapping", async () => {
    const wrapper = mount(SidebarViewSwitcher, {
      props: { mode: "all" }, // index 0 of [all, unified, favorite, tags, recent]
      attachTo: document.body,
    });
    const radios = wrapper.findAll('[role="radio"]');
    expect(radios[0]!.attributes("tabindex")).toBe("0");
    expect(radios[1]!.attributes("tabindex")).toBe("-1");

    await radios[0]!.trigger("keydown", { key: "ArrowRight" });
    expect(wrapper.emitted("select")?.at(-1)).toEqual(["unified"]);

    // ArrowLeft from the first wraps to the last mode.
    await radios[0]!.trigger("keydown", { key: "ArrowLeft" });
    expect(wrapper.emitted("select")?.at(-1)).toEqual(["recent"]);
    wrapper.unmount();
  });
});
