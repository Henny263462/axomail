import { computed, onUnmounted, ref, watch } from "vue";

import { searchMessages } from "../api/search";
import type { SearchResultDto } from "../types/mail";

/** Debounce window before a typed query is sent to the backend. */
const DEBOUNCE_MS = 250;
/** Maximum number of hits requested per search. */
const RESULT_LIMIT = 30;

/**
 * Owns the message-search state for a list pane: the live query, the debounced
 * request, and the ranked results.
 *
 * The query updates immediately (so the pane can switch into results mode and
 * the input stays responsive), but the backend call is debounced by
 * {@link DEBOUNCE_MS}. A monotonic sequence guards against out-of-order
 * responses so a slow earlier request can never overwrite a newer one. No
 * message bodies are retained — only the snippet-bearing hit envelopes.
 */
export function useSearch() {
  const query = ref("");
  const results = ref<SearchResultDto[]>([]);
  const searching = ref(false);

  let timer: ReturnType<typeof setTimeout> | null = null;
  let sequence = 0;

  /** True once the trimmed query has content; drives results mode. */
  const active = computed(() => query.value.trim().length > 0);
  const resultCount = computed(() => results.value.length);

  function cancelTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  async function run(text: string): Promise<void> {
    const id = ++sequence;
    searching.value = true;
    const hits = await searchMessages(text, RESULT_LIMIT);
    if (id !== sequence) return;
    results.value = hits ?? [];
    searching.value = false;
  }

  watch(query, (next) => {
    cancelTimer();
    const trimmed = next.trim();
    if (trimmed === "") {
      sequence += 1;
      results.value = [];
      searching.value = false;
      return;
    }
    // Enter the pending state now so the pane reads "Searching…" through the
    // debounce window rather than briefly flashing an empty-results message.
    searching.value = true;
    timer = setTimeout(() => void run(trimmed), DEBOUNCE_MS);
  });

  /** Clears the query and any pending request, returning to list mode. */
  function clear(): void {
    cancelTimer();
    sequence += 1;
    query.value = "";
    results.value = [];
    searching.value = false;
  }

  onUnmounted(cancelTimer);

  return { query, results, resultCount, active, searching, clear };
}
