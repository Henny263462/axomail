import { describe, expect, it } from "vitest";

import { computeWindow } from "../src/composables/useVirtualList";

const ROW = 76;
const VIEWPORT = 480;
const OVERSCAN = 4;

describe("computeWindow", () => {
  it("renders only a small window of a huge list", () => {
    const count = 100_000;
    const win = computeWindow(0, VIEWPORT, ROW, count, OVERSCAN);
    expect(win.start).toBe(0);
    // ceil(480/76)+1 = 8 visible rows, plus overscan below.
    expect(win.end).toBe(8 + OVERSCAN);
    expect(win.end - win.start).toBeLessThan(20);
    expect(win.totalHeight).toBe(count * ROW);
    expect(win.offsetY).toBe(0);
  });

  it("advances the window and offset as the list scrolls", () => {
    const count = 1000;
    const scrollTop = 100 * ROW; // row 100 at the top
    const win = computeWindow(scrollTop, VIEWPORT, ROW, count, OVERSCAN);
    expect(win.start).toBe(100 - OVERSCAN);
    expect(win.offsetY).toBe((100 - OVERSCAN) * ROW);
    expect(win.end).toBeGreaterThan(100);
    expect(win.end).toBeLessThan(120);
  });

  it("clamps an over-scroll to the last rows", () => {
    const count = 50;
    const win = computeWindow(1_000_000, VIEWPORT, ROW, count, OVERSCAN);
    expect(win.end).toBe(count);
    expect(win.start).toBeLessThan(count);
  });

  it("is empty for an empty list", () => {
    const win = computeWindow(0, VIEWPORT, ROW, 0, OVERSCAN);
    expect(win).toEqual({ start: 0, end: 0, offsetY: 0, totalHeight: 0 });
  });
});
