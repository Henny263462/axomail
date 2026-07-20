<script setup lang="ts">
/**
 * The condition-row editor for the advanced-search builder: dynamic field / op /
 * value rows, each addable and removable. The conditions array is a two-way
 * model, so the dialog owns the data and this component keeps the builder SFC
 * under the file-size gate.
 */
import { useI18n } from "vue-i18n";

import type { SearchCondition } from "../../types/search";
import { FIELDS, defaultOpFor, fieldKind, opsFor } from "./searchOptions";

const conditions = defineModel<SearchCondition[]>({ required: true });

const { t } = useI18n({ useScope: "global" });

function addCondition(): void {
  conditions.value.push({ field: "subject", op: "contains", value: "" });
}
function removeCondition(index: number): void {
  conditions.value.splice(index, 1);
}

/** Realigns a row's op and value to its (possibly changed) field kind. */
function onFieldChange(row: SearchCondition): void {
  if (!opsFor(row.field).some((o) => o.value === row.op)) {
    row.op = defaultOpFor(row.field);
  }
  const kind = fieldKind(row.field);
  if (kind === "bool" && row.value !== "true" && row.value !== "false") {
    row.value = "true";
  } else if (kind === "date" && !/^\d{4}-\d{2}-\d{2}$/.test(row.value)) {
    row.value = "";
  }
}
</script>

<template>
  <fieldset class="rows">
    <legend>{{ t("search.dialog.conditions") }}</legend>
    <div
      v-for="(row, index) in conditions"
      :key="index"
      class="row"
    >
      <select
        v-model="row.field"
        :aria-label="t('search.dialog.conditionField')"
        @change="onFieldChange(row)"
      >
        <option
          v-for="f in FIELDS"
          :key="f.value"
          :value="f.value"
        >
          {{ t(f.labelKey) }}
        </option>
      </select>
      <select
        v-model="row.op"
        :aria-label="t('search.dialog.conditionOperator')"
      >
        <option
          v-for="o in opsFor(row.field)"
          :key="o.value"
          :value="o.value"
        >
          {{ t(o.labelKey) }}
        </option>
      </select>
      <select
        v-if="fieldKind(row.field) === 'bool'"
        v-model="row.value"
        :aria-label="t('search.dialog.conditionValue')"
      >
        <option value="true">
          {{ t("search.dialog.yes") }}
        </option>
        <option value="false">
          {{ t("search.dialog.no") }}
        </option>
      </select>
      <input
        v-else
        v-model="row.value"
        :type="fieldKind(row.field) === 'date' ? 'date' : 'text'"
        :aria-label="t('search.dialog.conditionValue')"
      >
      <button
        type="button"
        class="icon"
        :aria-label="t('search.dialog.removeCondition')"
        @click="removeCondition(index)"
      >
        ×
      </button>
    </div>
    <button
      type="button"
      class="link"
      @click="addCondition"
    >
      {{ t("search.dialog.addCondition") }}
    </button>
  </fieldset>
</template>

<style scoped>
.rows {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: var(--space-2);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.rows legend {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
}

.row {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex-wrap: wrap;
}

.row select,
.row input {
  flex: 1;
  min-width: 100px;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: var(--text-sm);
}

.icon {
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-danger);
  width: 26px;
  height: 26px;
  flex: none;
}

.link {
  border: none;
  background: none;
  color: var(--color-accent);
  font-size: var(--text-xs);
  align-self: flex-start;
  cursor: pointer;
}
</style>
