/**
 * A dependency-free fixed-row-height windowing primitive.
 *
 * It renders only the rows intersecting the viewport (plus a small overscan),
 * so a mailbox with 100k messages scrolls in O(viewport) DOM nodes. The row
 * height is uniform, so the window is pure arithmetic over `scrollTop`; the
 * caller spaces the rendered slice with `offsetY` and reserves the full scroll
 * extent with `totalHeight`.
 *
 * The windowing math ({@link computeWindow}) is exported and unit-tested on its
 * own; the composable adds only the reactive glue (scroll + resize tracking and
 * programmatic scroll-to for keyboard navigation).
 */
import {
  computed,
  onMounted,
  onUnmounted,
  ref,
  type ComputedRef,
  type Ref,
} from "vue";

/** The slice of rows to render plus the geometry to position them. */
export interface VirtualWindow {
  /** First rendered row index (inclusive). */
  start: number;
  /** One past the last rendered row index (exclusive). */
  end: number;
  /** Pixel offset of the first rendered row from the top of the content. */
  offsetY: number;
  /** Total scroll height of every row. */
  totalHeight: number;
}

/**
 * A fallback viewport height (px) used before a real measurement lands (and in
 * layout-less test environments), so a window is always computed rather than
 * collapsing to zero rows.
 */
export const FALLBACK_VIEWPORT = 480;

/** Pure window arithmetic for a fixed row height. */
export function computeWindow(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  count: number,
  overscan: number,
): VirtualWindow {
  const totalHeight = count * rowHeight;
  if (count === 0 || rowHeight <= 0) {
    return { start: 0, end: 0, offsetY: 0, totalHeight };
  }
  const clampedTop = Math.max(0, Math.min(scrollTop, Math.max(0, totalHeight - 1)));
  const firstVisible = Math.floor(clampedTop / rowHeight);
  const visibleCount = Math.ceil(viewportHeight / rowHeight) + 1;
  const start = Math.max(0, firstVisible - overscan);
  const end = Math.min(count, firstVisible + visibleCount + overscan);
  return { start, end, offsetY: start * rowHeight, totalHeight };
}

/** Options for {@link useVirtualList}. */
export interface VirtualListOptions {
  /** Reactive total row count. */
  count: Ref<number> | ComputedRef<number>;
  /** Uniform row height in pixels. */
  rowHeight: number;
  /** Extra rows rendered above and below the viewport (default 4). */
  overscan?: number;
}

/** Reactive windowing over a scroll container. */
export function useVirtualList(options: VirtualListOptions) {
  const overscan = options.overscan ?? 4;
  const scroller = ref<HTMLElement | null>(null);
  const scrollTop = ref(0);
  const viewportHeight = ref(FALLBACK_VIEWPORT);

  const window = computed(() =>
    computeWindow(
      scrollTop.value,
      viewportHeight.value,
      options.rowHeight,
      options.count.value,
      overscan,
    ),
  );

  function onScroll(event: Event): void {
    scrollTop.value = (event.target as HTMLElement).scrollTop;
  }

  function measure(): void {
    const height = scroller.value?.clientHeight ?? 0;
    if (height > 0) viewportHeight.value = height;
  }

  /**
   * Scrolls so row `index` is fully in view, adjusting the container's
   * `scrollTop` and the tracked value together so the window recomputes at once
   * (keyboard navigation relies on this).
   */
  function scrollToIndex(index: number): void {
    const el = scroller.value;
    const height = viewportHeight.value;
    const top = index * options.rowHeight;
    const bottom = top + options.rowHeight;
    let next = scrollTop.value;
    if (top < next) next = top;
    else if (bottom > next + height) next = bottom - height;
    if (next === scrollTop.value) return;
    scrollTop.value = next;
    if (el) el.scrollTop = next;
  }

  let observer: ResizeObserver | null = null;
  onMounted(() => {
    measure();
    if (scroller.value && typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => measure());
      observer.observe(scroller.value);
    }
  });
  onUnmounted(() => {
    observer?.disconnect();
    observer = null;
  });

  return { scroller, scrollTop, viewportHeight, window, onScroll, scrollToIndex };
}
