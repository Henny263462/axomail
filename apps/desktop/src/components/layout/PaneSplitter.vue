<script setup lang="ts">
/**
 * A draggable, keyboard-operable pane splitter (`role="separator"`).
 *
 * Pointer: press-drag along the splitter's cross axis emits `resize` with the
 * new absolute size (`start size + drag delta`); the parent clamps and persists.
 * Keyboard: with the separator focused, the arrow keys along its axis nudge the
 * size (Shift = larger step), and Home/End jump to the min/max. `aria-valuenow`
 * always reflects the live size so assistive tech can announce the change.
 *
 * `orientation` describes the separator itself: a `vertical` separator sits
 * between side-by-side panes (drag left/right → change width); a `horizontal`
 * one sits between stacked panes (drag up/down → change height).
 */
import { ref } from "vue";

const props = withDefaults(
  defineProps<{
    /** The separator's own orientation. */
    orientation: "vertical" | "horizontal";
    /** Current size of the pane the splitter resizes, in pixels. */
    value: number;
    /** Inclusive lower bound (for `aria-valuemin`). */
    min: number;
    /** Inclusive upper bound (for `aria-valuemax`). */
    max: number;
    /** Accessible name (what this splitter resizes). */
    label: string;
    /** Keyboard step in pixels; Shift multiplies it. */
    step?: number;
    reverse?: boolean;
  }>(),
  { step: 16, reverse: false },
);

const emit = defineEmits<{
  /** A new absolute pixel size the parent should clamp and apply. */
  resize: [value: number];
}>();

const dragging = ref(false);
/** Pointer coordinate and pane size captured at drag start. */
let startCoord = 0;
let startValue = 0;

function axisCoord(event: PointerEvent): number {
  return props.orientation === "vertical" ? event.clientX : event.clientY;
}

function onPointerdown(event: PointerEvent): void {
  // Only the primary button starts a drag.
  if (event.button !== 0) return;
  event.preventDefault();
  dragging.value = true;
  startCoord = axisCoord(event);
  startValue = props.value;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function onPointermove(event: PointerEvent): void {
  if (!dragging.value) return;
  const delta = axisCoord(event) - startCoord;
  emit("resize", startValue + (props.reverse ? -delta : delta));
}

function endDrag(event: PointerEvent): void {
  if (!dragging.value) return;
  dragging.value = false;
  const target = event.currentTarget as HTMLElement;
  if (target.hasPointerCapture(event.pointerId)) {
    target.releasePointerCapture(event.pointerId);
  }
}

function onKeydown(event: KeyboardEvent): void {
  const step = event.shiftKey ? props.step * 4 : props.step;
  const before = props.orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
  const after = props.orientation === "vertical" ? "ArrowRight" : "ArrowDown";
  const decrease = props.reverse ? after : before;
  const increase = props.reverse ? before : after;
  let next: number | null = null;
  if (event.key === decrease) next = props.value - step;
  else if (event.key === increase) next = props.value + step;
  else if (event.key === "Home") next = props.min;
  else if (event.key === "End") next = props.max;
  if (next === null) return;
  event.preventDefault();
  emit("resize", next);
}
</script>

<template>
  <div
    class="splitter"
    :class="[orientation, { dragging }]"
    role="separator"
    tabindex="0"
    :aria-orientation="orientation"
    :aria-label="label"
    :aria-valuenow="value"
    :aria-valuemin="min"
    :aria-valuemax="max"
    @pointerdown="onPointerdown"
    @pointermove="onPointermove"
    @pointerup="endDrag"
    @pointercancel="endDrag"
    @keydown="onKeydown"
  >
    <span
      class="grip"
      aria-hidden="true"
    />
  </div>
</template>

<style scoped>
.splitter {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  touch-action: none;
  transition: background var(--duration-base) ease;
}

.splitter::before {
  content: "";
  position: absolute;
  background: var(--color-border);
  transition: background var(--duration-base) ease;
}

.splitter.vertical::before {
  inset: 0 calc(50% - 0.5px);
}

.splitter.horizontal::before {
  inset: calc(50% - 0.5px) 0;
}

.splitter.vertical {
  cursor: col-resize;
}

.splitter.horizontal {
  cursor: row-resize;
}

.splitter:hover,
.splitter.dragging {
  background: var(--color-accent-soft);
}

.splitter:hover::before,
.splitter.dragging::before {
  background: var(--color-accent);
}

.splitter:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: -1px;
}

/* A faint centered grip so the thin bar is discoverable. */
.grip {
  position: relative;
  background: var(--color-border-strong);
  border-radius: 2px;
  opacity: 0;
  transition: opacity var(--duration-base) ease;
}

.splitter:hover .grip,
.splitter.dragging .grip,
.splitter:focus-visible .grip {
  opacity: 1;
}

.splitter.vertical .grip {
  width: 2px;
  height: 24px;
}

.splitter.horizontal .grip {
  width: 24px;
  height: 2px;
}

.splitter:hover .grip,
.splitter.dragging .grip {
  background: var(--color-accent);
}

@media (prefers-reduced-motion: reduce) {
  .splitter {
    transition: none;
  }
}
</style>
