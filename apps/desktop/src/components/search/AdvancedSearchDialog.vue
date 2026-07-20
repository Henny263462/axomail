<script setup lang="ts">
/**
 * The advanced / faceted search builder: a multi-condition query (match all or
 * any of a set of field/op/value rows — the rows live in
 * {@link AdvancedSearchConditions}), a folder scope, and a sort. "Search" runs
 * the query and hands the results back to the shell; "Save this search" persists
 * it as a sidebar virtual folder. Reuses the filter model's condition ops via
 * the shared {@link ../../types/search} representation. Focus is trapped and
 * Escape cancels, via the shared {@link useFocusTrap}.
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import {
  createSavedSearch,
  runAdvancedSearch,
  updateSavedSearch,
} from "../../api/savedSearch";
import { useFocusTrap } from "../../composables/useFocusTrap";
import type { AccountSummary, SearchResultDto } from "../../types/mail";
import type {
  SavedSearchDto,
  SearchCondition,
  SearchMatchType,
  SearchScopeKind,
  SearchSort,
} from "../../types/search";
import AdvancedSearchConditions from "./AdvancedSearchConditions.vue";
import { SCOPES, SORTS } from "./searchOptions";

const props = defineProps<{
  savedSearch: SavedSearchDto | null;
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
  run: [payload: { title: string; results: SearchResultDto[] }];
  saved: [];
}>();

const { t } = useI18n({ useScope: "global" });

const root = ref<HTMLElement | null>(null);
let invoker: HTMLElement | null = null;
const { onKeydown } = useFocusTrap(root, () => emit("close"));

const draft = reactive({
  name: "",
  matchType: "all" as SearchMatchType,
  conditions: [] as SearchCondition[],
  scopeKind: "all_folders" as SearchScopeKind,
  scopeMailboxId: null as number | null,
  sort: "date_desc" as SearchSort,
});

const error = ref<string | null>(null);
const busy = ref(false);

/** A flat list of every account's folders for the scope picker. */
const folders = computed(() =>
  props.accounts.flatMap((account) =>
    account.mailboxes.map((mailbox) => ({
      id: mailbox.id,
      label: `${account.displayName} · ${mailbox.name}`,
    })),
  ),
);

const needsFolder = computed(() => draft.scopeKind !== "all_folders");
const canSave = computed(() => draft.name.trim() !== "" && !busy.value);
const canSearch = computed(
  () => !busy.value && (!needsFolder.value || draft.scopeMailboxId !== null),
);

watch(() => props.savedSearch, reset, { immediate: true });

function reset(): void {
  const s = props.savedSearch;
  draft.name = s?.name ?? "";
  draft.matchType = s?.query.matchType ?? "all";
  draft.conditions = s
    ? s.query.conditions.map((c) => ({ ...c }))
    : [{ field: "subject", op: "contains", value: "" }];
  draft.scopeKind = s?.query.scope.kind ?? "all_folders";
  draft.scopeMailboxId = s?.query.scope.mailboxId ?? null;
  draft.sort = s?.query.sort ?? "date_desc";
}

onMounted(() => {
  invoker = document.activeElement as HTMLElement | null;
  root.value?.querySelector<HTMLInputElement>(".name-input")?.focus();
});
onBeforeUnmount(() => invoker?.focus());

/** Auto-selects the first folder when switching to a folder-scoped search. */
function onScopeChange(): void {
  if (needsFolder.value && draft.scopeMailboxId === null) {
    draft.scopeMailboxId = folders.value[0]?.id ?? null;
  }
}

function buildQuery() {
  return {
    matchType: draft.matchType,
    conditions: draft.conditions.map((c) => ({ ...c })),
    scope: {
      kind: draft.scopeKind,
      mailboxId: needsFolder.value ? draft.scopeMailboxId : null,
    },
    sort: draft.sort,
  };
}

async function onSearch(): Promise<void> {
  error.value = null;
  busy.value = true;
  try {
    const hits = await runAdvancedSearch(buildQuery());
    emit("run", { title: draft.name.trim(), results: hits ?? [] });
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function onSave(): Promise<void> {
  error.value = null;
  busy.value = true;
  try {
    const input = { name: draft.name.trim(), query: buildQuery() };
    if (props.savedSearch) await updateSavedSearch(props.savedSearch.id, input);
    else await createSavedSearch(input);
    emit("saved");
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="advanced-search-title"
    >
      <h3
        id="advanced-search-title"
        class="title"
      >
        {{ t("search.dialog.title") }}
      </h3>

      <label class="field">
        <span>{{ t("search.dialog.name") }}</span>
        <input
          v-model="draft.name"
          class="name-input"
          type="text"
          :aria-label="t('search.dialog.name')"
          :placeholder="t('search.dialog.namePlaceholder')"
        >
      </label>

      <label class="field">
        <span>{{ t("search.dialog.match") }}</span>
        <select
          v-model="draft.matchType"
          :aria-label="t('search.dialog.match')"
        >
          <option value="all">
            {{ t("search.dialog.matchAll") }}
          </option>
          <option value="any">
            {{ t("search.dialog.matchAny") }}
          </option>
        </select>
      </label>

      <AdvancedSearchConditions v-model="draft.conditions" />

      <label class="field">
        <span>{{ t("search.dialog.scope") }}</span>
        <select
          v-model="draft.scopeKind"
          :aria-label="t('search.dialog.scope')"
          @change="onScopeChange"
        >
          <option
            v-for="s in SCOPES"
            :key="s.value"
            :value="s.value"
          >
            {{ t(s.labelKey) }}
          </option>
        </select>
      </label>
      <label
        v-if="needsFolder"
        class="field"
      >
        <span>&nbsp;</span>
        <select
          v-model.number="draft.scopeMailboxId"
          :aria-label="t('search.dialog.chooseFolder')"
        >
          <option
            :value="null"
            disabled
          >
            {{ t("search.dialog.chooseFolder") }}
          </option>
          <option
            v-for="folder in folders"
            :key="folder.id"
            :value="folder.id"
          >
            {{ folder.label }}
          </option>
        </select>
      </label>

      <label class="field">
        <span>{{ t("search.dialog.sort") }}</span>
        <select
          v-model="draft.sort"
          :aria-label="t('search.dialog.sort')"
        >
          <option
            v-for="s in SORTS"
            :key="s.value"
            :value="s.value"
          >
            {{ t(s.labelKey) }}
          </option>
        </select>
      </label>

      <p
        v-if="error !== null"
        class="error"
        role="alert"
      >
        {{ error }}
      </p>

      <div class="buttons">
        <button
          type="button"
          class="action primary"
          :disabled="!canSearch"
          @click="onSearch"
        >
          {{ t("search.dialog.search") }}
        </button>
        <button
          type="button"
          class="action"
          :disabled="!canSave"
          @click="onSave"
        >
          {{ props.savedSearch ? t("search.dialog.update") : t("search.dialog.save") }}
        </button>
        <button
          type="button"
          class="action"
          @click="emit('close')"
        >
          {{ t("search.dialog.close") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 560px;
  max-height: 88vh;
  overflow-y: auto;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
}

.field {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.field > span {
  min-width: 84px;
  color: var(--color-text-secondary);
}

.field input,
.field select {
  flex: 1;
  min-width: 100px;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: var(--text-sm);
}

.error {
  font-size: var(--text-sm);
  color: var(--color-danger);
}

.buttons {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

.action {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  font-size: var(--text-sm);
  cursor: pointer;
}

.action.primary {
  color: var(--color-accent);
}

.action:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
