<script setup lang="ts">
/**
 * The add/edit form for one filter: name, match-type, stop-on-match, and
 * dynamic condition and action rows (each addable/removable). Emits an assembled
 * {@link FilterInput} on save; the parent section persists it.
 */
import { reactive, watch } from "vue";
import { useI18n } from "vue-i18n";

import type {
  FilterAction,
  FilterActionKind,
  FilterCondition,
  FilterConditionField,
  FilterConditionOp,
  FilterDto,
  FilterInput,
  MailboxSummary,
} from "../../types/mail";

const props = defineProps<{
  filter: FilterDto | null;
  accountId: number | null;
  folders: MailboxSummary[];
}>();

const emit = defineEmits<{
  save: [input: FilterInput];
  cancel: [];
}>();

const { t } = useI18n({ useScope: "global" });

const FIELDS: { value: FilterConditionField; labelKey: string }[] = [
  { value: "from", labelKey: "settings.filters.field.from" },
  { value: "to", labelKey: "settings.filters.field.to" },
  { value: "cc", labelKey: "settings.filters.field.cc" },
  { value: "subject", labelKey: "settings.filters.field.subject" },
  { value: "body", labelKey: "settings.filters.field.body" },
  { value: "tag", labelKey: "settings.filters.field.tag" },
  { value: "has_attachment", labelKey: "settings.filters.field.hasAttachment" },
  { value: "size", labelKey: "settings.filters.field.size" },
  { value: "date", labelKey: "settings.filters.field.date" },
];

const TEXT_OPS: { value: FilterConditionOp; labelKey: string }[] = [
  { value: "contains", labelKey: "settings.filters.op.contains" },
  { value: "not_contains", labelKey: "settings.filters.op.notContains" },
  { value: "is", labelKey: "settings.filters.op.is" },
  { value: "is_not", labelKey: "settings.filters.op.isNot" },
  { value: "starts_with", labelKey: "settings.filters.op.startsWith" },
  { value: "ends_with", labelKey: "settings.filters.op.endsWith" },
  { value: "matches_regex", labelKey: "settings.filters.op.matchesRegex" },
];
const NUMBER_OPS: { value: FilterConditionOp; labelKey: string }[] = [
  { value: "greater_than", labelKey: "settings.filters.op.greaterThan" },
  { value: "less_than", labelKey: "settings.filters.op.lessThan" },
  { value: "is", labelKey: "settings.filters.op.is" },
  { value: "is_not", labelKey: "settings.filters.op.isNot" },
];
const BOOL_OPS: { value: FilterConditionOp; labelKey: string }[] = [
  { value: "is", labelKey: "settings.filters.op.is" },
  { value: "is_not", labelKey: "settings.filters.op.isNot" },
];

const ACTIONS: { value: FilterActionKind; labelKey: string }[] = [
  { value: "move", labelKey: "settings.filters.action.move" },
  { value: "copy", labelKey: "settings.filters.action.copy" },
  { value: "tag", labelKey: "settings.filters.action.tag" },
  { value: "star", labelKey: "settings.filters.action.star" },
  { value: "mark_read", labelKey: "settings.filters.action.markRead" },
  { value: "mark_unread", labelKey: "settings.filters.action.markUnread" },
  { value: "mark_junk", labelKey: "settings.filters.action.markJunk" },
  { value: "delete_to_trash", labelKey: "settings.filters.action.deleteToTrash" },
  { value: "forward", labelKey: "settings.filters.action.forward" },
  { value: "stop", labelKey: "settings.filters.action.stop" },
];

const draft = reactive({
  name: "",
  matchType: "all" as "all" | "any",
  stopOnMatch: false,
  conditions: [] as FilterCondition[],
  actions: [] as FilterAction[],
});

watch(() => props.filter, reset, { immediate: true });

function reset(): void {
  const f = props.filter;
  draft.name = f?.name ?? "";
  draft.matchType = f?.matchType ?? "all";
  draft.stopOnMatch = f?.stopOnMatch ?? false;
  draft.conditions = f
    ? f.conditions.map((c) => ({ ...c }))
    : [{ field: "subject", op: "contains", value: "" }];
  draft.actions = f ? f.actions.map((a) => ({ ...a })) : [{ kind: "star", param: "" }];
}

/** Which op set a field offers. */
function opsFor(field: FilterConditionField): { value: FilterConditionOp; labelKey: string }[] {
  if (field === "size" || field === "date") return NUMBER_OPS;
  if (field === "has_attachment") return BOOL_OPS;
  return TEXT_OPS;
}

function onFieldChange(row: FilterCondition): void {
  const ops = opsFor(row.field);
  if (!ops.some((o) => o.value === row.op) && ops[0]) row.op = ops[0].value;
}

/** Whether an action carries a param input, and its kind. */
function paramKind(kind: FilterActionKind): "folder" | "tag" | "email" | "none" {
  if (kind === "move" || kind === "copy") return "folder";
  if (kind === "tag") return "tag";
  if (kind === "forward") return "email";
  return "none";
}

function addCondition(): void {
  draft.conditions.push({ field: "subject", op: "contains", value: "" });
}
function removeCondition(index: number): void {
  draft.conditions.splice(index, 1);
}
function addAction(): void {
  draft.actions.push({ kind: "star", param: "" });
}
function removeAction(index: number): void {
  draft.actions.splice(index, 1);
}

function save(): void {
  emit("save", {
    accountId: props.accountId,
    name: draft.name,
    enabled: props.filter?.enabled ?? true,
    matchType: draft.matchType,
    stopOnMatch: draft.stopOnMatch,
    conditions: draft.conditions.map((c) => ({ ...c })),
    actions: draft.actions.map((a) => ({ ...a })),
  });
}
</script>

<template>
  <form
    class="editor"
    :aria-label="t('settings.filters.editorLabel')"
    @submit.prevent="save"
  >
    <label class="field">
      <span>{{ t("common.name") }}</span>
      <input
        v-model="draft.name"
        type="text"
        :aria-label="t('settings.filters.nameLabel')"
        :placeholder="t('settings.filters.namePlaceholder')"
      >
    </label>

    <label class="field">
      <span>{{ t("settings.filters.match") }}</span>
      <select
        v-model="draft.matchType"
        :aria-label="t('settings.filters.matchTypeLabel')"
      >
        <option value="all">
          {{ t("settings.filters.matchAll") }}
        </option>
        <option value="any">
          {{ t("settings.filters.matchAny") }}
        </option>
      </select>
    </label>

    <fieldset class="rows">
      <legend>{{ t("settings.filters.conditions") }}</legend>
      <div
        v-for="(row, index) in draft.conditions"
        :key="index"
        class="row"
      >
        <select
          v-model="row.field"
          :aria-label="t('settings.filters.conditionField')"
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
          :aria-label="t('settings.filters.conditionOperator')"
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
          v-if="row.field === 'has_attachment'"
          v-model="row.value"
          :aria-label="t('settings.filters.conditionValue')"
        >
          <option value="true">
            {{ t("settings.filters.yes") }}
          </option>
          <option value="false">
            {{ t("settings.filters.no") }}
          </option>
        </select>
        <input
          v-else
          v-model="row.value"
          :type="row.field === 'date' ? 'date' : row.field === 'size' ? 'number' : 'text'"
          :aria-label="t('settings.filters.conditionValue')"
        >
        <button
          type="button"
          class="icon"
          :aria-label="t('settings.filters.removeCondition')"
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
        {{ t("settings.filters.addCondition") }}
      </button>
    </fieldset>

    <fieldset class="rows">
      <legend>{{ t("settings.filters.actions") }}</legend>
      <div
        v-for="(row, index) in draft.actions"
        :key="index"
        class="row"
      >
        <select
          v-model="row.kind"
          :aria-label="t('settings.filters.actionLabel')"
        >
          <option
            v-for="a in ACTIONS"
            :key="a.value"
            :value="a.value"
          >
            {{ t(a.labelKey) }}
          </option>
        </select>
        <select
          v-if="paramKind(row.kind) === 'folder'"
          v-model="row.param"
          :aria-label="t('settings.filters.targetFolder')"
        >
          <option
            value=""
            disabled
          >
            {{ t("settings.filters.chooseFolder") }}
          </option>
          <option
            v-for="folder in folders"
            :key="folder.id"
            :value="String(folder.id)"
          >
            {{ folder.name }}
          </option>
        </select>
        <input
          v-else-if="paramKind(row.kind) === 'tag'"
          v-model="row.param"
          type="text"
          :aria-label="t('settings.filters.tagName')"
          :placeholder="t('settings.filters.tagName')"
        >
        <input
          v-else-if="paramKind(row.kind) === 'email'"
          v-model="row.param"
          type="email"
          :aria-label="t('settings.filters.forwardAddressLabel')"
          :placeholder="t('settings.filters.forwardPlaceholder')"
        >
        <button
          type="button"
          class="icon"
          :aria-label="t('settings.filters.removeAction')"
          @click="removeAction(index)"
        >
          ×
        </button>
      </div>
      <button
        type="button"
        class="link"
        @click="addAction"
      >
        {{ t("settings.filters.addAction") }}
      </button>
    </fieldset>

    <label class="check">
      <input
        v-model="draft.stopOnMatch"
        type="checkbox"
      >
      <span>{{ t("settings.filters.stopHint") }}</span>
    </label>

    <div class="buttons">
      <button
        type="submit"
        class="action"
        :disabled="draft.name.trim() === ''"
      >
        {{ t("settings.filters.saveFilter") }}
      </button>
      <button
        type="button"
        class="action"
        @click="emit('cancel')"
      >
        {{ t("common.cancel") }}
      </button>
    </div>
  </form>
</template>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  background: var(--color-bg);
}

.field,
.check,
.buttons {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

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

.field input,
.field select,
.row select,
.row input,
.action {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: var(--text-sm);
}

.field input,
.field select,
.row input {
  flex: 1;
  min-width: 100px;
}

.icon {
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-danger);
  width: 26px;
  height: 26px;
}

.link {
  border: none;
  background: none;
  color: var(--color-accent);
  font-size: var(--text-xs);
  align-self: flex-start;
  cursor: pointer;
}

.action {
  color: var(--color-accent);
}

.action:disabled {
  opacity: 0.5;
}
</style>
