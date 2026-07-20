/**
 * Typed wrappers over the advanced-search and saved-search Tauri commands.
 *
 * Reads and saved-search runs use {@link safeInvoke} (resolving to `null` in the
 * browser preview where the IPC bridge is absent, ADR-0005); an ad-hoc advanced
 * run and the mutations use {@link invokeOrThrow} so a backend validation error
 * (a bad regex, a folder scope with no folder) reaches the dialog's error
 * surface.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type { SearchResultDto } from "../types/mail";
import type { SavedSearchDto, SavedSearchInput, SearchQuery } from "../types/search";

/** Runs an ad-hoc advanced query. Rejects with the backend error on failure. */
export function runAdvancedSearch(
  query: SearchQuery,
): Promise<SearchResultDto[] | null> {
  return invokeOrThrow<SearchResultDto[]>("run_advanced_search", { query });
}

/** Lists every saved search, alphabetically. */
export function listSavedSearches(): Promise<SavedSearchDto[] | null> {
  return safeInvoke<SavedSearchDto[]>("list_saved_searches");
}

/** Creates a saved search. Rejects on failure. */
export function createSavedSearch(
  input: SavedSearchInput,
): Promise<SavedSearchDto | null> {
  return invokeOrThrow<SavedSearchDto>("create_saved_search", { input });
}

/** Replaces a saved search's name and query. Rejects on failure. */
export function updateSavedSearch(
  savedSearchId: number,
  input: SavedSearchInput,
): Promise<SavedSearchDto | null> {
  return invokeOrThrow<SavedSearchDto>("update_saved_search", {
    savedSearchId,
    input,
  });
}

/** Deletes a saved search. Rejects on failure. */
export function deleteSavedSearch(savedSearchId: number): Promise<null> {
  return invokeOrThrow<null>("delete_saved_search", { savedSearchId });
}

/** Runs a saved search by id. */
export function runSavedSearch(
  savedSearchId: number,
): Promise<SearchResultDto[] | null> {
  return safeInvoke<SearchResultDto[]>("run_saved_search", { savedSearchId });
}
