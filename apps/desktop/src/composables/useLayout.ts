import { computed, ref, type ComputedRef } from "vue";

import type { Pane } from "./usePaneNavigation";

export type PaneLayout = "classic" | "vertical" | "wide" | "focus";

export const PANE_LAYOUTS: readonly PaneLayout[] = ["classic", "vertical", "wide", "focus"];

export const DEFAULT_PANE_ORDER: readonly Pane[] = ["sidebar", "list", "message"];

const STORAGE_KEY = "axomail.layout";
const ORDER_KEY = "axomail.paneOrder";

const layout = ref<PaneLayout>("vertical");
const paneOrder = ref<Pane[]>([...DEFAULT_PANE_ORDER]);

function isLayout(value: string | null): value is PaneLayout {
  return value !== null && (PANE_LAYOUTS as readonly string[]).includes(value);
}

function isPaneOrder(value: unknown): value is Pane[] {
  return (
    Array.isArray(value) &&
    value.length === DEFAULT_PANE_ORDER.length &&
    DEFAULT_PANE_ORDER.every((pane) => value.includes(pane))
  );
}

function readStored(): PaneLayout {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY) ?? null;
    if (isLayout(raw)) return raw;
  } catch {
    return "vertical";
  }
  return "vertical";
}

function readStoredOrder(): Pane[] {
  try {
    const raw = globalThis.localStorage?.getItem(ORDER_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (isPaneOrder(parsed)) return [...parsed];
  } catch {
    return [...DEFAULT_PANE_ORDER];
  }
  return [...DEFAULT_PANE_ORDER];
}

function write(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch {
    return;
  }
}

type TransitionDocument = Document & {
  startViewTransition?: (update: () => void) => unknown;
};

export function withViewTransition(update: () => void): void {
  const doc = globalThis.document as TransitionDocument | undefined;
  const reduced = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  if (doc && typeof doc.startViewTransition === "function" && !reduced) {
    doc.startViewTransition(update);
  } else {
    update();
  }
}

export function initLayout(): void {
  layout.value = readStored();
  paneOrder.value = readStoredOrder();
}

export function setLayout(next: PaneLayout): void {
  layout.value = next;
  write(STORAGE_KEY, next);
}

export function cycleLayout(): void {
  const index = PANE_LAYOUTS.indexOf(layout.value);
  setLayout(PANE_LAYOUTS[(index + 1) % PANE_LAYOUTS.length] ?? "vertical");
}

function commitOrder(next: Pane[]): void {
  paneOrder.value = next;
  write(ORDER_KEY, JSON.stringify(next));
}

export function swapPanes(a: Pane, b: Pane): void {
  if (a === b) return;
  commitOrder(paneOrder.value.map((pane) => (pane === a ? b : pane === b ? a : pane)));
}

export function movePaneBy(pane: Pane, delta: number): void {
  const visible = visibleOrder(layout.value, paneOrder.value);
  const target = visible[visible.indexOf(pane) + delta];
  if (target !== undefined) swapPanes(pane, target);
}

export function resetPaneOrder(): void {
  commitOrder([...DEFAULT_PANE_ORDER]);
}

export function toggleFocusLayout(): void {
  setLayout(layout.value === "focus" ? "vertical" : "focus");
}

export function visibleOrder(current: PaneLayout, order: readonly Pane[]): Pane[] {
  return current === "focus" ? order.filter((pane) => pane !== "sidebar") : [...order];
}

export function useLayout(): {
  layout: ComputedRef<PaneLayout>;
  paneOrder: ComputedRef<Pane[]>;
  isDefaultOrder: ComputedRef<boolean>;
  setLayout: (next: PaneLayout) => void;
  cycleLayout: () => void;
  swapPanes: (a: Pane, b: Pane) => void;
  movePaneBy: (pane: Pane, delta: number) => void;
  resetPaneOrder: () => void;
  toggleFocusLayout: () => void;
} {
  return {
    layout: computed(() => layout.value),
    paneOrder: computed(() => paneOrder.value),
    isDefaultOrder: computed(() =>
      paneOrder.value.every((pane, index) => pane === DEFAULT_PANE_ORDER[index]),
    ),
    setLayout,
    cycleLayout,
    swapPanes,
    movePaneBy,
    resetPaneOrder,
    toggleFocusLayout,
  };
}
