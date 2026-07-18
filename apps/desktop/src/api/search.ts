/**
 * Typed wrapper over the `search_messages` Tauri command.
 *
 * Full-text search spans every account and returns ranked hits, each with a
 * highlighted snippet. Resolves to `null` in the browser preview where the
 * IPC bridge is absent (ADR-0005); the caller treats that as no results.
 */
import { safeInvoke } from "../lib/tauri";
import type { SearchResultDto } from "../types/mail";

/**
 * Full-text searches stored messages, newest-relevant first.
 *
 * @param query Free-text query; an empty or token-less query yields no hits.
 * @param limit Maximum number of hits to return.
 */
export function searchMessages(
  query: string,
  limit: number,
): Promise<SearchResultDto[] | null> {
  return safeInvoke<SearchResultDto[]>("search_messages", { query, limit });
}
