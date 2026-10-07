import { onBeforeUnmount, reactive, ref, type Ref } from "vue";

import type { Pane } from "./usePaneNavigation";

const THRESHOLD = 4;

function isPane(value: string | undefined): value is Pane {
  return value === "sidebar" || value === "list" || value === "message";
}

function paneAt(x: number, y: number): Pane | null {
  const element = globalThis.document?.elementFromPoint?.(x, y);
  const host = element?.closest<HTMLElement>("[data-pane]");
  const pane = host?.dataset.pane;
  return isPane(pane) ? pane : null;
}

export function usePaneDrag(onDrop: (from: Pane, to: Pane) => void): {
  dragging: Ref<Pane | null>;
  over: Ref<Pane | null>;
  pointer: { x: number; y: number };
  start: (event: PointerEvent, pane: Pane) => void;
} {
  const dragging = ref<Pane | null>(null);
  const over = ref<Pane | null>(null);
  const pointer = reactive({ x: 0, y: 0 });
  let source: Pane | null = null;
  let originX = 0;
  let originY = 0;

  function onMove(event: PointerEvent): void {
    if (source === null) return;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    if (dragging.value === null) {
      if (Math.hypot(event.clientX - originX, event.clientY - originY) < THRESHOLD) return;
      dragging.value = source;
    }
    const target = paneAt(event.clientX, event.clientY);
    over.value = target !== null && target !== source ? target : null;
  }

  function finish(commit: boolean): void {
    const from = dragging.value;
    const to = over.value;
    source = null;
    dragging.value = null;
    over.value = null;
    detach();
    if (commit && from !== null && to !== null) onDrop(from, to);
  }

  function onUp(): void {
    finish(true);
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") finish(false);
  }

  function attach(): void {
    globalThis.addEventListener("pointermove", onMove);
    globalThis.addEventListener("pointerup", onUp);
    globalThis.addEventListener("pointercancel", onUp);
    globalThis.addEventListener("keydown", onKey);
  }

  function detach(): void {
    globalThis.removeEventListener("pointermove", onMove);
    globalThis.removeEventListener("pointerup", onUp);
    globalThis.removeEventListener("pointercancel", onUp);
    globalThis.removeEventListener("keydown", onKey);
  }

  function start(event: PointerEvent, pane: Pane): void {
    if (event.button !== 0) return;
    event.preventDefault();
    source = pane;
    originX = event.clientX;
    originY = event.clientY;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    attach();
  }

  onBeforeUnmount(detach);

  return { dragging, over, pointer, start };
}
