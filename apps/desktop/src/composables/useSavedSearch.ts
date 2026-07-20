/**
 * Advanced-search results + saved-search state for the shell, extracted from
 * `App.vue` so the shell stays under the file-size gate.
 *
 * It owns the results shown in the `search` view, the sidebar's saved-search
 * list, and the entry points that switch into results mode. Following the
 * established props-down deps pattern, it reads/writes the reactive refs `App.vue`
 * owns (`activeView`, `selectedMessageId`) and reaches the shell through
 * callbacks. It also registers the "Advanced search" command against the Stage-0
 * command registry (a shortcut + a palette entry).
 */
import { ref, type Ref } from "vue";
import { useI18n } from "vue-i18n";

import { runSavedSearch } from "../api/savedSearch";
import type { AppDialogMap } from "../components/layout/dialogs";
import type { SidebarView } from "../components/sidebar/view";
import type { SearchResultDto } from "../types/mail";
import type { SavedSearchDto } from "../types/search";
import { useOptionalCommandRegistry } from "./useCommandRegistry";
import type { DialogController } from "./useDialogs";

/** The App-level state and collaborators this composable reads and drives. */
interface Deps {
  /** The active sidebar view; set to `"search"` when showing results. */
  activeView: Ref<SidebarView>;
  /** The open message; cleared when a new result set is shown. */
  selectedMessageId: Ref<number | null>;
  /** App dialog controller, used to open the advanced-search dialog. */
  dialogs: DialogController<AppDialogMap>;
  /** Reveals the list pane on the narrow, stacked layout. */
  showList: () => void;
  /** True while a blocking modal is open; gates the Advanced-search command. */
  isModalOpen: () => boolean;
}

export function useSavedSearch(deps: Deps) {
  const { t } = useI18n({ useScope: "global" });

  const results = ref<SearchResultDto[]>([]);
  const activeSavedSearchId = ref<number | null>(null);
  const resultsTitle = ref("");
  const running = ref(false);
  /** Bumped so the sidebar group (which owns the list) reloads after a change. */
  const refreshKey = ref(0);

  // Guards a slow saved-search run from overwriting a newer result set.
  let sequence = 0;

  /** Triggers the sidebar saved-search group to reload (after a save/delete). */
  function refresh(): void {
    refreshKey.value += 1;
  }

  /** Switches the shell into the results view. */
  function enterResultsView(title: string, savedId: number | null): void {
    resultsTitle.value = title || t("search.results.title");
    activeSavedSearchId.value = savedId;
    deps.activeView.value = "search";
    deps.selectedMessageId.value = null;
    deps.showList();
  }

  /** Shows results produced by the advanced-search dialog. */
  function showResults(payload: {
    title: string;
    results: SearchResultDto[];
  }): void {
    sequence += 1; // Invalidate any in-flight saved-search run.
    results.value = payload.results;
    running.value = false;
    enterResultsView(payload.title, null);
  }

  /** Runs and shows a saved search picked from the sidebar. */
  async function openSaved(saved: SavedSearchDto): Promise<void> {
    const id = ++sequence;
    running.value = true;
    results.value = [];
    enterResultsView(saved.name, saved.id);
    const hits = await runSavedSearch(saved.id);
    if (id !== sequence) return;
    results.value = hits ?? [];
    running.value = false;
  }

  /** Opens the advanced-search dialog, fresh or editing an existing search. */
  function openDialog(saved: SavedSearchDto | null = null): void {
    deps.dialogs.open("advancedSearch", { savedSearch: saved });
  }

  // Register the "Advanced search" command (a shortcut + a palette entry). The
  // optional registry keeps a scope-less unit mount from throwing.
  useOptionalCommandRegistry()?.register({
    id: "search.advanced",
    title: t("search.command"),
    keywords: ["find", "filter", "query", "builder", "faceted"],
    shortcut: "Mod+Shift+F",
    run: () => openDialog(null),
    isEnabled: () => !deps.isModalOpen(),
  });

  return {
    results,
    activeSavedSearchId,
    resultsTitle,
    running,
    refreshKey,
    refresh,
    showResults,
    openSaved,
    openDialog,
  };
}
