import { describe, expect, it, vi } from "vitest";

import {
  createChordDispatcher,
  formatChord,
  isChordSequence,
  parseChordSequence,
  type ChordBinding,
} from "../src/composables/useKeyChords";

/** Builds a keydown; `target` defaults to the document body (non-editable). */
function keydown(
  init: KeyboardEventInit,
  target: EventTarget = document.body,
): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { cancelable: true, ...init });
  Object.defineProperty(event, "target", { value: target, configurable: true });
  return event;
}

/** A clock the tests advance by hand for deterministic prefix expiry. */
function fakeClock(start = 0) {
  let t = start;
  return { now: () => t, advance: (ms: number) => (t += ms) };
}

describe("chord sequence parsing", () => {
  it("recognizes whitespace sequences and not modifier chords", () => {
    expect(isChordSequence("g i")).toBe(true);
    expect(isChordSequence("g   f")).toBe(true);
    expect(isChordSequence("Mod+K")).toBe(false);
    expect(isChordSequence("n")).toBe(false);
  });

  it("parses and lowercases a sequence, empty for non-sequences", () => {
    expect(parseChordSequence("g I")).toEqual(["g", "i"]);
    expect(parseChordSequence("Mod+K")).toEqual([]);
  });

  it("formats a sequence for display", () => {
    expect(formatChord(["g", "i"])).toBe("G then I");
  });
});

describe("createChordDispatcher", () => {
  function setup(bindings: ChordBinding[], clock = fakeClock()) {
    const dispatcher = createChordDispatcher({
      bindings: () => bindings,
      timeoutMs: 1000,
      now: clock.now,
    });
    return { dispatcher, clock };
  }

  it("runs a two-key chord and prevents the completing key's default", () => {
    const run = vi.fn();
    const { dispatcher } = setup([{ sequence: ["g", "i"], run }]);

    // The prefix key alone does not run or preventDefault.
    const prefix = keydown({ key: "g" });
    expect(dispatcher.handleKeydown(prefix)).toBe(false);
    expect(prefix.defaultPrevented).toBe(false);

    const complete = keydown({ key: "i" });
    expect(dispatcher.handleKeydown(complete)).toBe(true);
    expect(run).toHaveBeenCalledOnce();
    expect(complete.defaultPrevented).toBe(true);
  });

  it("expires a pending prefix after the timeout", () => {
    const run = vi.fn();
    const { dispatcher, clock } = setup([{ sequence: ["g", "i"], run }]);

    dispatcher.handleKeydown(keydown({ key: "g" }));
    clock.advance(1500); // beyond the 1000ms window
    expect(dispatcher.handleKeydown(keydown({ key: "i" }))).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });

  it("treats a non-matching second key as a fresh prefix", () => {
    const goInbox = vi.fn();
    const { dispatcher } = setup([
      { sequence: ["g", "i"], run: goInbox },
      { sequence: ["g", "f"], run: vi.fn() },
    ]);

    dispatcher.handleKeydown(keydown({ key: "g" }));
    // `g x` matches nothing; `x` is not a prefix, so it just resets.
    expect(dispatcher.handleKeydown(keydown({ key: "x" }))).toBe(false);
    // A brand-new `g i` still works.
    dispatcher.handleKeydown(keydown({ key: "g" }));
    expect(dispatcher.handleKeydown(keydown({ key: "i" }))).toBe(true);
    expect(goInbox).toHaveBeenCalledOnce();
  });

  it("does not start a chord while typing in a text field", () => {
    const run = vi.fn();
    const { dispatcher } = setup([{ sequence: ["g", "i"], run }]);
    const input = document.createElement("input");

    dispatcher.handleKeydown(keydown({ key: "g" }, input));
    dispatcher.handleKeydown(keydown({ key: "i" }, input));
    expect(run).not.toHaveBeenCalled();
  });

  it("ignores an event another handler already claimed", () => {
    const run = vi.fn();
    const { dispatcher } = setup([{ sequence: ["g", "i"], run }]);
    dispatcher.handleKeydown(keydown({ key: "g" }));
    const complete = keydown({ key: "i" });
    complete.preventDefault();
    expect(dispatcher.handleKeydown(complete)).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });

  it("cancels a pending prefix when a modifier chord follows", () => {
    const run = vi.fn();
    const { dispatcher } = setup([{ sequence: ["g", "i"], run }]);
    dispatcher.handleKeydown(keydown({ key: "g" }));
    // Ctrl+I is not a chord completion; the prefix is dropped.
    expect(dispatcher.handleKeydown(keydown({ key: "i", ctrlKey: true }))).toBe(
      false,
    );
    expect(run).not.toHaveBeenCalled();
  });

  it("does not fire a disabled chord", () => {
    const run = vi.fn();
    const { dispatcher } = setup([
      { sequence: ["g", "i"], run, isEnabled: () => false },
    ]);
    dispatcher.handleKeydown(keydown({ key: "g" }));
    expect(dispatcher.handleKeydown(keydown({ key: "i" }))).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });
});
