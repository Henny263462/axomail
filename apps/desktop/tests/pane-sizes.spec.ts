import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "../src/components/layout/AppShell.vue";
import LayoutSwitcher from "../src/components/layout/LayoutSwitcher.vue";
import PaneSplitter from "../src/components/layout/PaneSplitter.vue";
import { shellGrid } from "../src/components/layout/shellGrid";
import {
  cycleLayout,
  initLayout,
  movePaneBy,
  resetPaneOrder,
  setLayout,
  swapPanes,
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
  it("persists a chosen layout and cycles through all four", () => {
    const { layout } = useLayout();
    setLayout("classic");
    expect(layout.value).toBe("classic");
    expect(localStorage.getItem("axomail.layout")).toBe("classic");
    cycleLayout();
    expect(layout.value).toBe("vertical");
    cycleLayout();
    expect(layout.value).toBe("wide");
    cycleLayout();
    expect(layout.value).toBe("focus");
    cycleLayout();
    expect(layout.value).toBe("classic");
  });

  it("swaps, moves, persists and resets the pane order", () => {
    const { paneOrder, isDefaultOrder } = useLayout();
    swapPanes("sidebar", "message");
    expect(paneOrder.value).toEqual(["message", "list", "sidebar"]);
    expect(isDefaultOrder.value).toBe(false);
    expect(JSON.parse(localStorage.getItem("axomail.paneOrder") ?? "[]")).toEqual([
      "message",
      "list",
      "sidebar",
    ]);
    movePaneBy("message", 1);
    expect(paneOrder.value).toEqual(["list", "message", "sidebar"]);
    movePaneBy("list", -1);
    expect(paneOrder.value).toEqual(["list", "message", "sidebar"]);
    initLayout();
    expect(paneOrder.value).toEqual(["list", "message", "sidebar"]);
    resetPaneOrder();
    expect(isDefaultOrder.value).toBe(true);
  });

  it("ignores a corrupt stored pane order", () => {
    localStorage.setItem("axomail.paneOrder", JSON.stringify(["list", "list", "x"]));
    initLayout();
    expect(useLayout().paneOrder.value).toEqual(["sidebar", "list", "message"]);
  });
});

describe("shellGrid", () => {
  it("places panes by order and keeps one flexible column", () => {
    const grid = shellGrid("vertical", ["message", "sidebar", "list"]);
    expect(grid.template).toContain('"message splitA sidebar splitB list"');
    expect(grid.template.match(/1fr\)/g)).toHaveLength(1);
    expect(grid.splitters).toEqual([
      { area: "splitA", orientation: "vertical", dimension: "sidebarWidth", reverse: true },
      { area: "splitB", orientation: "vertical", dimension: "listWidth", reverse: true },
    ]);
  });

  it("keeps the default vertical behaviour", () => {
    const grid = shellGrid("vertical", ["sidebar", "list", "message"]);
    expect(grid.splitters.map((split) => [split.dimension, split.reverse])).toEqual([
      ["sidebarWidth", false],
      ["listWidth", false],
    ]);
  });

  it("drops the folder pane in the focus layout", () => {
    const grid = shellGrid("focus", ["list", "sidebar", "message"]);
    expect(grid.template).toContain('"list splitB message"');
    expect(grid.template).not.toContain("sidebar");
    expect(grid.splitters).toHaveLength(1);
  });

  it("puts the middle pane on top in the wide layout", () => {
    const grid = shellGrid("wide", ["sidebar", "message", "list"]);
    expect(grid.template).toContain('"message message message"');
    expect(grid.template).toContain('"sidebar splitA list"');
  });
});

describe("LayoutSwitcher", () => {
  it("marks the active layout and switches on click", async () => {
    setLayout("vertical");
    const wrapper = mount(LayoutSwitcher);
    const buttons = wrapper.findAll('[role="radio"]');
    expect(buttons).toHaveLength(4);
    const wide = buttons.find((b) => b.attributes("aria-label") === "Wide");
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

  it("shows a single splitter and hides the folder pane in focus", () => {
    stubMatchMedia(false);
    setLayout("focus");
    const wrapper = mount(AppShell, { slots });
    expect(wrapper.findAll('[role="separator"]')).toHaveLength(1);
    expect(wrapper.get(".col-sidebar").attributes("style")).toContain("display: none");
  });

  it("swaps panes when a grip is dragged onto another pane", async () => {
    stubMatchMedia(false);
    setLayout("vertical");
    const wrapper = mount(AppShell, { slots, attachTo: document.body });
    const target = wrapper.get(".col-message").element;
    document.elementFromPoint = () => target;
    await wrapper
      .get(".col-sidebar .pane-grip")
      .trigger("pointerdown", { button: 0, clientX: 10, clientY: 10 });
    window.dispatchEvent(new MouseEvent("pointermove", { clientX: 400, clientY: 200 }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find(".col-message .drop-hint").exists()).toBe(true);
    window.dispatchEvent(new MouseEvent("pointerup", { clientX: 400, clientY: 200 }));
    expect(useLayout().paneOrder.value).toEqual(["message", "list", "sidebar"]);
    wrapper.unmount();
  });

  it("moves a pane with the arrow keys on its grip", async () => {
    stubMatchMedia(false);
    setLayout("vertical");
    const wrapper = mount(AppShell, { slots });
    await wrapper.get(".col-list .pane-grip").trigger("keydown", { key: "ArrowLeft" });
    expect(useLayout().paneOrder.value).toEqual(["list", "sidebar", "message"]);
  });

  it("hides the splitters on a narrow, stacked viewport", () => {
    stubMatchMedia(true);
    setLayout("vertical");
    const wrapper = mount(AppShell, { slots });
    expect(wrapper.findAll('[role="separator"]')).toHaveLength(0);
  });
});
