import { computed, onBeforeUnmount, ref } from "vue";

export type Pane = "sidebar" | "list" | "message";

/** Below this viewport width the three columns collapse into a stack. */
const STACK_BREAKPOINT = "(max-width: 880px)";

/**
 * Layout state for the three-column shell: on narrow viewports only one
 * pane is visible at a time and navigation switches between them.
 */
export function usePaneNavigation() {
  const query =
    typeof window.matchMedia === "function"
      ? window.matchMedia(STACK_BREAKPOINT)
      : null;

  const isStacked = ref(query?.matches ?? false);
  const activePane = ref<Pane>("list");

  const onChange = (event: MediaQueryListEvent) => {
    isStacked.value = event.matches;
  };
  query?.addEventListener("change", onChange);
  onBeforeUnmount(() => query?.removeEventListener("change", onChange));

  function showPane(pane: Pane) {
    activePane.value = pane;
  }

  const paneVisible = computed(() => (pane: Pane) => {
    return isStacked.value ? activePane.value === pane : true;
  });

  return { isStacked, activePane, showPane, paneVisible };
}
