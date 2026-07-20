import { describe, expect, it } from "vitest";

import {
  commandGroupLabel,
  formatShortcutHint,
  groupCommands,
  shortcutGroups,
} from "../src/commands/display";
import {
  filterCommands,
  fuzzyMatch,
  highlightSegments,
} from "../src/components/command/fuzzy";
import type { Command } from "../src/types/command";

/** A command with sensible defaults, overridable per test. */
function cmd(over: Partial<Command> & { id: string }): Command {
  return { title: over.id, keywords: [], run: () => {}, ...over };
}

describe("fuzzyMatch", () => {
  it("matches an in-order subsequence, case-insensitively", () => {
    expect(fuzzyMatch("nm", "New message")).not.toBeNull();
    expect(fuzzyMatch("xyz", "New message")).toBeNull();
  });

  it("scores a contiguous prefix above a scattered match", () => {
    const contiguous = fuzzyMatch("rep", "Reply")!;
    const scattered = fuzzyMatch("rep", "Recipient pane")!;
    expect(contiguous.score).toBeGreaterThan(scattered.score);
  });

  it("returns the matched ranges for highlighting", () => {
    const result = fuzzyMatch("re", "Reply")!;
    expect(result.ranges).toEqual([[0, 2]]);
  });

  it("matches everything for an empty query", () => {
    expect(fuzzyMatch("", "anything")).toEqual({ score: 0, ranges: [] });
  });
});

describe("highlightSegments", () => {
  it("splits a title into matched and unmatched runs", () => {
    expect(highlightSegments("Reply", [[0, 2]])).toEqual([
      { text: "Re", match: true },
      { text: "ply", match: false },
    ]);
  });

  it("returns one plain segment when nothing matched", () => {
    expect(highlightSegments("Reply", [])).toEqual([
      { text: "Reply", match: false },
    ]);
  });
});

describe("filterCommands", () => {
  const commands: Command[] = [
    cmd({ id: "compose.new", title: "New message", keywords: ["write"] }),
    cmd({ id: "message.reply", title: "Reply" }),
    cmd({ id: "message.replyAll", title: "Reply all" }),
    cmd({ id: "nav.focusSearch", title: "Search messages", keywords: ["find"] }),
  ];

  it("returns every command in order for an empty query", () => {
    expect(filterCommands(commands, "").map((r) => r.command.id)).toEqual(
      commands.map((c) => c.id),
    );
  });

  it("narrows and ranks by relevance", () => {
    const ids = filterCommands(commands, "reply").map((r) => r.command.id);
    expect(ids).toEqual(["message.reply", "message.replyAll"]);
  });

  it("matches on a keyword even when the title does not", () => {
    const ids = filterCommands(commands, "write").map((r) => r.command.id);
    expect(ids).toEqual(["compose.new"]);
  });

  it("drops commands that do not match at all", () => {
    expect(filterCommands(commands, "zzz")).toEqual([]);
  });
});

describe("formatShortcutHint", () => {
  it("renders a chord sequence as a `then` phrase", () => {
    expect(formatShortcutHint("g i")).toBe("G then I");
  });

  it("renders the help alias as a bare question mark", () => {
    expect(formatShortcutHint("Shift+?")).toBe("?");
  });

  it("delegates a modifier chord to the registry formatter", () => {
    expect(formatShortcutHint("Mod+K")).toBe("Ctrl+K");
  });

  it("is empty for no shortcut", () => {
    expect(formatShortcutHint(undefined)).toBe("");
  });
});

describe("grouping", () => {
  it("labels a namespace, capitalising unknown ones", () => {
    expect(commandGroupLabel("message.reply")).toBe("Message");
    expect(commandGroupLabel("widgets.foo")).toBe("Widgets");
  });

  it("buckets commands by namespace in first-seen order", () => {
    const groups = groupCommands([
      cmd({ id: "message.reply" }),
      cmd({ id: "nav.goInbox" }),
      cmd({ id: "message.forward" }),
    ]);
    expect(groups.map((g) => g.id)).toEqual(["message", "nav"]);
    expect(groups[0]!.commands.map((c) => c.id)).toEqual([
      "message.reply",
      "message.forward",
    ]);
  });

  it("shortcutGroups keeps only commands that have a shortcut", () => {
    const groups = shortcutGroups([
      cmd({ id: "message.reply", shortcut: "r" }),
      cmd({ id: "message.noKey" }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.commands.map((c) => c.id)).toEqual(["message.reply"]);
  });
});
