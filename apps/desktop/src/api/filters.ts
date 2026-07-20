/**
 * Typed wrappers over the message-filter Tauri commands.
 *
 * Reads use {@link safeInvoke} (resolving to `null` in the browser preview
 * where the IPC bridge is absent, ADR-0005); mutations use {@link invokeOrThrow}
 * so a backend validation error (bad regex, missing target folder) reaches the
 * settings section's error surface.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  AccountFilterSettings,
  FilterDto,
  FilterInput,
  FilterRunOutcome,
} from "../types/mail";

/** Lists the filters in a scope: an account, or the global rules (`null`). */
export function listFilters(
  accountId: number | null,
): Promise<FilterDto[] | null> {
  return safeInvoke<FilterDto[]>("list_filters", { accountId });
}

/** Creates a filter. Rejects with the backend error string on failure. */
export function createFilter(input: FilterInput): Promise<FilterDto | null> {
  return invokeOrThrow<FilterDto>("create_filter", { input });
}

/** Replaces a filter's content. Rejects on failure. */
export function updateFilter(
  filterId: number,
  input: FilterInput,
): Promise<FilterDto | null> {
  return invokeOrThrow<FilterDto>("update_filter", { filterId, input });
}

/** Deletes a filter. Rejects on failure. */
export function deleteFilter(filterId: number): Promise<null> {
  return invokeOrThrow<null>("delete_filter", { filterId });
}

/** Enables or disables a filter. Rejects on failure. */
export function setFilterEnabled(
  filterId: number,
  enabled: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_filter_enabled", { filterId, enabled });
}

/** Reorders a scope's filters to match `orderedIds`. Rejects on failure. */
export function reorderFilters(
  accountId: number | null,
  orderedIds: number[],
): Promise<null> {
  return invokeOrThrow<null>("reorder_filters", { accountId, orderedIds });
}

/** Runs filters over a folder now. Rejects on failure. */
export function runFiltersOnFolder(
  mailboxId: number,
): Promise<FilterRunOutcome | null> {
  return invokeOrThrow<FilterRunOutcome>("run_filters_on_folder", {
    mailboxId,
  });
}

/** Reads an account's run-on-arrival filtering setting. */
export function getAccountFilterSettings(
  accountId: number,
): Promise<AccountFilterSettings | null> {
  return safeInvoke<AccountFilterSettings>("get_account_filter_settings", {
    accountId,
  });
}

/** Sets an account's run-on-arrival filtering toggle. Rejects on failure. */
export function setAccountFilterArrival(
  accountId: number,
  enabled: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_account_filter_arrival", {
    accountId,
    enabled,
  });
}
