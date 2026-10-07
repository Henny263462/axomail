import type { PaneLayout } from "../../composables/useLayout";
import type { PaneDimension } from "../../composables/usePaneSizes";
import type { Pane } from "../../composables/usePaneNavigation";

export interface SplitterSpec {
  area: "splitA" | "splitB" | "splitC";
  orientation: "vertical" | "horizontal";
  dimension: PaneDimension;
  reverse: boolean;
}

export interface ShellGrid {
  template: string;
  splitters: SplitterSpec[];
}

const SPLIT = "var(--split)";

function width(pane: Pane): string {
  if (pane === "sidebar") return "var(--sidebar-width)";
  if (pane === "list") return "var(--list-width)";
  return "minmax(0, 1fr)";
}

function dimension(pane: Pane): PaneDimension {
  return pane === "sidebar" ? "sidebarWidth" : "listWidth";
}

function between(
  left: Pane,
  right: Pane,
  area: SplitterSpec["area"],
  messageBefore = false,
): SplitterSpec {
  return left === "message" || messageBefore
    ? { area, orientation: "vertical", dimension: dimension(right), reverse: true }
    : { area, orientation: "vertical", dimension: dimension(left), reverse: false };
}

function pairColumns(left: Pane, right: Pane): string {
  return left === "message"
    ? `minmax(0, 1fr) ${SPLIT} ${width(right)}`
    : `${width(left)} ${SPLIT} minmax(0, 1fr)`;
}

const heightSplit: SplitterSpec = {
  area: "splitC",
  orientation: "horizontal",
  dimension: "listHeight",
  reverse: false,
};

export function shellGrid(layout: PaneLayout, order: readonly Pane[]): ShellGrid {
  const [a = "sidebar", b = "list", c = "message"] = order;

  if (layout === "focus") {
    const [left = "list", right = "message"] = order.filter((pane) => pane !== "sidebar");
    return {
      template: `"${left} splitB ${right}" 1fr / ${pairColumns(left, right)}`,
      splitters: [between(left, right, "splitB")],
    };
  }

  if (layout === "classic") {
    const columns =
      a === "message"
        ? `minmax(0, 1fr) ${SPLIT} var(--list-width)`
        : `${width(a)} ${SPLIT} minmax(0, 1fr)`;
    const side: SplitterSpec =
      a === "message"
        ? { area: "splitA", orientation: "vertical", dimension: "listWidth", reverse: true }
        : { area: "splitA", orientation: "vertical", dimension: dimension(a), reverse: false };
    return {
      template: `"${a} splitA ${b}" var(--list-height) "${a} splitA splitC" ${SPLIT} "${a} splitA ${c}" 1fr / ${columns}`,
      splitters: [side, heightSplit],
    };
  }

  if (layout === "wide") {
    return {
      template: `"${b} ${b} ${b}" var(--list-height) "splitC splitC splitC" ${SPLIT} "${a} splitA ${c}" 1fr / ${pairColumns(a, c)}`,
      splitters: [between(a, c, "splitA"), heightSplit],
    };
  }

  return {
    template: `"${a} splitA ${b} splitB ${c}" 1fr / ${width(a)} ${SPLIT} ${width(b)} ${SPLIT} ${width(c)}`,
    splitters: [between(a, b, "splitA"), between(b, c, "splitB", a === "message")],
  };
}
