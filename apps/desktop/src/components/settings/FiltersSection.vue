<script setup lang="ts">
/**
 * Message-filters settings section: per scope (an account or the global rules),
 * list the filters with an enable toggle and reorder controls, add/edit them
 * through {@link FilterEditor}, run them now over a chosen folder, and toggle
 * run-on-arrival filtering per account. Autonomous — persists through the filter
 * api and reports outcomes via the shared {@link SettingsSectionContext}.
 */
import { computed, ref, watch } from "vue";

import {
  createFilter,
  deleteFilter,
  getAccountFilterSettings,
  listFilters,
  reorderFilters,
  runFiltersOnFolder,
  setAccountFilterArrival,
  setFilterEnabled,
  updateFilter,
} from "../../api/filters";
import { isTauri } from "../../lib/tauri";
import type { FilterDto, FilterInput } from "../../types/mail";
import { messageFrom, type SettingsSectionContext } from "./context";
import FilterEditor from "./FilterEditor.vue";

const GLOBAL = -1;

const props = defineProps<{ context: SettingsSectionContext }>();

const scopeId = ref<number>(props.context.accounts[0]?.id ?? GLOBAL);
const filters = ref<FilterDto[]>([]);
const editing = ref<{ open: boolean; filter: FilterDto | null }>({ open: false, filter: null });
const arrivalEnabled = ref<boolean | null>(null);
const runFolderId = ref<number | null>(null);

/** The scope id passed to the backend: `null` for the global rules. */
const scope = computed<number | null>(() => (scopeId.value === GLOBAL ? null : scopeId.value));
const scopeAccount = computed(() =>
  props.context.accounts.find((account) => account.id === scopeId.value),
);
const folders = computed(() => scopeAccount.value?.mailboxes ?? []);

watch(scopeId, load, { immediate: true });

async function load(): Promise<void> {
  editing.value = { open: false, filter: null };
  filters.value = (await listFilters(scope.value)) ?? [];
  runFolderId.value = folders.value[0]?.id ?? null;
  arrivalEnabled.value =
    scope.value === null ? null : (await getAccountFilterSettings(scope.value))?.arrivalEnabled ?? null;
}

function guardTauri(): boolean {
  if (isTauri()) return true;
  props.context.reportError(
    "Managing filters needs the desktop app; it is unavailable in the browser preview.",
  );
  return false;
}

function startAdd(): void {
  editing.value = { open: true, filter: null };
}
function startEdit(filter: FilterDto): void {
  editing.value = { open: true, filter };
}

async function onSave(input: FilterInput): Promise<void> {
  props.context.reportStatus("");
  if (!guardTauri()) return;
  try {
    if (editing.value.filter) {
      await updateFilter(editing.value.filter.id, input);
      props.context.reportStatus(`Saved “${input.name}”.`);
    } else {
      await createFilter(input);
      props.context.reportStatus(`Created “${input.name}”.`);
    }
    props.context.reportError("");
    editing.value = { open: false, filter: null };
    await load();
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function toggle(filter: FilterDto): Promise<void> {
  if (!guardTauri()) return;
  try {
    await setFilterEnabled(filter.id, !filter.enabled);
    filter.enabled = !filter.enabled;
    props.context.reportError("");
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function remove(filter: FilterDto): Promise<void> {
  if (!guardTauri()) return;
  try {
    await deleteFilter(filter.id);
    filters.value = filters.value.filter((f) => f.id !== filter.id);
    props.context.reportStatus(`Deleted “${filter.name}”.`);
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function move(index: number, delta: number): Promise<void> {
  const next = index + delta;
  const reordered = [...filters.value];
  const current = reordered[index];
  const other = reordered[next];
  if (!current || !other) return;
  reordered[index] = other;
  reordered[next] = current;
  filters.value = reordered;
  if (!guardTauri()) return;
  try {
    await reorderFilters(
      scope.value,
      reordered.map((f) => f.id),
    );
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function onArrivalToggle(): Promise<void> {
  if (scope.value === null || arrivalEnabled.value === null || !guardTauri()) return;
  const next = !arrivalEnabled.value;
  try {
    await setAccountFilterArrival(scope.value, next);
    arrivalEnabled.value = next;
    props.context.reportError("");
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function runNow(): Promise<void> {
  if (runFolderId.value === null || !guardTauri()) return;
  props.context.reportStatus("");
  try {
    const outcome = await runFiltersOnFolder(runFolderId.value);
    if (outcome) {
      props.context.reportStatus(
        `Ran filters: ${outcome.matched} message(s) matched, ${outcome.actionsApplied} action(s) applied.`,
      );
    }
    props.context.reportError("");
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      Filters
    </legend>

    <label class="scope">
      <span>Rules for</span>
      <select
        v-model.number="scopeId"
        aria-label="Filter scope"
      >
        <option
          v-for="account in context.accounts"
          :key="account.id"
          :value="account.id"
        >
          {{ account.displayName }}
        </option>
        <option :value="GLOBAL">
          All accounts
        </option>
      </select>
    </label>

    <label
      v-if="arrivalEnabled !== null"
      class="check"
    >
      <input
        type="checkbox"
        :checked="arrivalEnabled"
        aria-label="Run filters on new mail"
        @change="onArrivalToggle"
      >
      <span>Run these filters automatically on newly-arrived mail</span>
    </label>

    <p
      v-if="filters.length === 0"
      class="empty"
    >
      No filters yet. Add one to sort mail automatically.
    </p>

    <ul
      v-else
      class="filter-list"
    >
      <li
        v-for="(filter, index) in filters"
        :key="filter.id"
        class="filter-row"
      >
        <input
          type="checkbox"
          :checked="filter.enabled"
          :aria-label="`Enable ${filter.name}`"
          @change="toggle(filter)"
        >
        <span class="name">{{ filter.name }}</span>
        <button
          type="button"
          class="icon"
          :aria-label="`Move ${filter.name} up`"
          :disabled="index === 0"
          @click="move(index, -1)"
        >
          ↑
        </button>
        <button
          type="button"
          class="icon"
          :aria-label="`Move ${filter.name} down`"
          :disabled="index === filters.length - 1"
          @click="move(index, 1)"
        >
          ↓
        </button>
        <button
          type="button"
          class="action"
          @click="startEdit(filter)"
        >
          Edit
        </button>
        <button
          type="button"
          class="danger"
          @click="remove(filter)"
        >
          Delete
        </button>
      </li>
    </ul>

    <FilterEditor
      v-if="editing.open"
      :filter="editing.filter"
      :account-id="scope"
      :folders="folders"
      @save="onSave"
      @cancel="editing = { open: false, filter: null }"
    />
    <button
      v-else
      type="button"
      class="action add"
      @click="startAdd"
    >
      Add filter
    </button>

    <div
      v-if="folders.length > 0"
      class="run-now"
    >
      <label>
        <span>Run now on</span>
        <select
          v-model.number="runFolderId"
          aria-label="Run filters on folder"
        >
          <option
            v-for="folder in folders"
            :key="folder.id"
            :value="folder.id"
          >
            {{ folder.name }}
          </option>
        </select>
      </label>
      <button
        type="button"
        class="action"
        @click="runNow"
      >
        Run now
      </button>
    </div>
  </fieldset>
</template>

<style scoped>
.group {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0 var(--space-1);
}

.scope,
.check,
.run-now {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.scope select,
.run-now select {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.empty {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.filter-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.filter-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.name {
  flex: 1;
  font-size: var(--text-sm);
  color: var(--color-text);
}

.icon,
.action,
.danger {
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  color: var(--color-accent);
}

.icon:disabled {
  opacity: 0.4;
}

.danger {
  border-color: var(--color-danger);
  color: var(--color-danger);
}

.add {
  align-self: flex-start;
}

.run-now {
  border-top: 1px solid var(--color-border);
  padding-top: var(--space-2);
}
</style>
