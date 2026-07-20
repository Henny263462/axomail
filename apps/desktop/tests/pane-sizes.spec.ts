import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "../src/components/layout/AppShell.vue";
import LayoutSwitcher from "../src/components/layout/LayoutSwitcher.vue";
import PaneSplitter from "../src/components/layout/PaneSplitter.vue";
import {
  cycleLayout,
  initLayout,
  setLayout,
  useLayout,
} from "../src/composables/useLayout";
import {
  initPaneSizes,
  PANE_BOUNDS,
  setPaneSize,
  usePaneSizes,
} from "../src/composables/usePaneSizes";

function stubMatchMedia(matches: boolean): void {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

beforeEach(() => {
  localStorage.clear();
  initPaneSizes();
  initLayout();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("usePaneSizes", () => {
  it("clamps a size to its bounds and persists it", () => {
    const { sizes } = usePaneSizes();
    setPaneSize("sidebarWidth", 10_000);
    expect(sizes.sidebarWidth).toBe(PANE_BOUNDS.sidebarWidth.max);
    setPaneSize("sidebarWidth", 0);
    expect(sizes.sidebarWidth).toBe(PANE_BOUNDS.sidebarWidth.min);

    const stored = JSON.parse(localStorage.getItem("axomail.paneSizes") ?? "{}");
    expect(stored.sidebarWidth).toBe(PANE_BOUNDS.sidebarWidth.min);
  });

  it("restores clamped sizes from a persisted cache", () => {
    localStorage.setItem(
      "axomail.paneSizes",
      JSON.stringify({ sidebarWidth: 300, listWidth: 99_999 }),
    );
    initPaneSizes();
    const { sizes, cssVars } = usePaneSizes();
    expect(sizes.sidebarWidth).toBe(300);
    // Out-of-range value clamped to the max.
    expect(sizes.listWidth).toBe(PANE_BOUNDS.listWidth.max);
    expect(cssVars.value["--sidebar-width"]).toBe("300px");
  });
});

describe("PaneSplitter", () => {
  function mountSplitter(orientation: "vertical" | "horizontal") {
    return mount(PaneSplitter, {
      props: {
        orientation,
        value: 300,
        min: 100,
        max: 500,
        label: "Resize",
        step: 16,
      },
    });
  }

  it("exposes separator semantics for assistive tech", () => {
    const wrapper = mountSplitter("vertical");
    const sep = wrapper.get('[role="separator"]');
    expect(sep.attributes("aria-orientation")).toBe("vertical");
    expect(sep.attributes("aria-valuenow")).toBe("300");
    expect(sep.attributes("aria-valuemin")).toBe("100");
    expect(sep.attributes("aria-valuemax")).toBe("500");
    expect(sep.attributes("tabindex")).toBe("0");
    expect(sep.attributes("aria-label")).toBe("Resize");
  });

  it("resizes with the arrow keys along its axis", async () => {
    const wrapper = mountSplitter("vertical");
    const sep = wrapper.get('[role="separator"]');
    await sep.trigger("keydown", { key: "ArrowRight" });
    await sep.trigger("keydown", { key: "ArrowLeft" });
    await sep.trigger("keydown", { key: "ArrowRight", shiftKey: true });
    await sep.trigger("keydown", { key: "Home" });
    await sep.trigger("keydown", { key: "End" });
    const emitted = wrapper.emitted("resize");
    expect(emitted).toEqual([[316], [284], [364], [100], [500]]);
  });

  it("uses the cross axis for a horizontal splitter", async () => {
    const wrapper = mountSplitter("horizontal");
    const sep = wrapper.get('[role="separator"]');
    // Left/Right do nothing on a horizontal separator.
    await sep.trigger("keydown", { key: "ArrowRight" });
    expect(wrapper.emitted("resize")).toBeUndefined();
    await sep.trigger("keydown", { key: "ArrowDown" });
    await sep.trigger("keydown", { key: "ArrowUp" });
    expect(wrapper.emitted("resize")).toEqual([[316], [284]]);
  });

  it("emits an absolute size while dragging", async () => {
    const wrapper = mountSplitter("vertical");
    const sep = wrapper.get('[role="separator"]');
    const el = sep.element as HTMLElement;
    el.setPointerCapture = () => {};
    el.releasePointerCapture = () => {};
    el.hasPointerCapture = () => true;
    await sep.trigger("pointerdown", { button: 0, clientX: 500, pointerId: 1 });
    await sep.trigger("pointermove", { clientX: 540, pointerId: 1 });
    // start value 300 + (540 - 500) = 340.
    expect(wrapper.emitted("resize")?.at(-1)).toEqual([340]);
  });
});

describe("useLayout", () => {
  it("persists a chosen layout and cycles through the three", () => {
    const { layout } = useLayout();
    setLayout("classic");
    expect(layout.value).toBe("classic");
    expect(localStorage.getItem("axomail.layout")).toBe("classic");
    cycleLayout();
    expect(layout.value).toBe("vertical");
    cycleLayout();
    expect(layout.value).toBe("wide");
    cycleLayout();
    expect(layout.value).toBe("classic");
  });
});

describe("LayoutSwitcher", () => {
  it("marks the active layout and switches on click", async () => {
    setLayout("vertical");
    const wrapper = mount(LayoutSwitcher);
    const buttons = wrapper.findAll('[role="radio"]');
    expect(buttons).toHaveLength(3);
    const wide = buttons.find((b) => b.text() === "Wide");
    if (wide === undefined) throw new Error("Wide button missing");
    await wide.trigger("click");
    expect(wide.attributes("aria-checked")).toBe("true");
    expect(useLayout().layout.value).toBe("wide");
  });
});

describe("AppShell layout + splitters", () => {
  const slots = {
    sidebar: "<div data-test='sidebar'>S</div>",
    list: "<div data-test='list'>L</div>",
    message: "<div data-test='message'>M</div>",
  };

  it("renders the chosen layout with the matching splitters", () => {
    stubMatchMedia(false);
    setLayout("vertical");
    const wrapper = mount(AppShell, { slots });
    expect(wrapper.get(".columns").classes()).toContain("vertical");
    // Vertical: two vertical splitters (sidebar + list).
    const separators = wrapper.findAll('[role="separator"]');
    expect(separators).toHaveLength(2);
    for (const sep of separators) {
      expect(sep.attributes("aria-orientation")).toBe("vertical");
    }
  });

  it("switches to a horizontal splitter in the classic layout", () => {
    stubMatchMedia(false);
    setLayout("classic");
    const wrapper = mount(AppShell, { slots });
    expect(wrapper.get(".columns").classes()).toContain("classic");
    const separators = wrapper.findAll('[role="separator"]');
    expect(separators).toHaveLength(2);
    const orientations = separators
      .map((s) => s.attributes("aria-orientation"))
      .sort();
    expect(orientations).toEqual(["horizontal", "vertical"]);
  });

  it("hides the splitters on a narrow, stacked viewport", () => {
    stubMatchMedia(true);
    setLayout("vertical");
    const wrapper = mount(AppShell, { slots });
    expect(wrapper.findAll('[role="separator"]')).toHaveLength(0);
  });
});
