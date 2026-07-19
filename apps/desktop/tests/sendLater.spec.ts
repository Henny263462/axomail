import { describe, expect, it } from "vitest";

import {
  datetimeLocalToUnix,
  formatScheduleTime,
  SEND_LATER_PRESETS,
} from "../src/lib/sendLater";

describe("SEND_LATER_PRESETS", () => {
  it("all resolve to a future whole-second instant", () => {
    const now = Date.now();
    for (const preset of SEND_LATER_PRESETS) {
      const at = preset.at(now);
      expect(Number.isInteger(at)).toBe(true);
      expect(at).toBeGreaterThan(Math.floor(now / 1000));
    }
  });

  it("computes 'in 1 hour' as now plus 3600 seconds", () => {
    const now = Date.UTC(2026, 6, 19, 12, 0, 0);
    const oneHour = SEND_LATER_PRESETS.find((p) => p.id === "in-1-hour");
    expect(oneHour?.at(now)).toBe(Math.floor(now / 1000) + 3600);
  });
});

describe("datetimeLocalToUnix", () => {
  it("returns null for an empty value", () => {
    expect(datetimeLocalToUnix("")).toBeNull();
  });

  it("parses a local datetime into whole seconds", () => {
    const value = "2026-07-19T14:30";
    const expected = Math.floor(new Date(value).getTime() / 1000);
    expect(datetimeLocalToUnix(value)).toBe(expected);
  });

  it("returns null for an unparseable value", () => {
    expect(datetimeLocalToUnix("not-a-date")).toBeNull();
  });
});

describe("formatScheduleTime", () => {
  it("produces a non-empty localized string", () => {
    expect(formatScheduleTime(Math.floor(Date.now() / 1000)).length).toBeGreaterThan(0);
  });
});
