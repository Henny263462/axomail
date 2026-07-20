<script setup lang="ts">
/**
 * The add/edit form for one filter: name, match-type, stop-on-match, and
 * dynamic condition and action rows (each addable/removable). Emits an assembled
 * {@link FilterInput} on save; the parent section persists it.
 */
import { reactive, watch } from "vue";

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

const FIELDS: { value: FilterConditionField; label: string }[] = [
  { value: "from", label: "From" },
  { value: "to", label: "To" },
  { value: "cc", label: "Cc" },
  { value: "subject", label: "Subject" },
  { value: "body", label: "Body" },
  { value: "tag", label: "Tag" },
  { value: "has_attachment", label: "Has attachment" },
  { value: "size", label: "Size (bytes)" },
  { value: "date", label: "Date" },
];

const TEXT_OPS: { value: FilterConditionOp; label: string }[] = [
  { value: "contains", label: "contains" },
  { value: "not_contains", label: "does not contain" },
  { value: "is", label: "is" },
  { value: "is_not", label: "is not" },
  { value: "starts_with", label: "starts with" },
  { value: "ends_with", label: "ends with" },
  { value: "matches_regex", label: "matches regex" },
];
const NUMBER_OPS: { value: FilterConditionOp; label: string }[] = [
  { value: "greater_than", label: "greater than" },
  { value: "less_than", label: "less than" },
  { value: "is", label: "is" },
  { value: "is_not", label: "is not" },
];
const BOOL_OPS: { value: FilterConditionOp; label: string }[] = [
  { value: "is", label: "is" },
  { value: "is_not", label: "is not" },
];

const ACTIONS: { value: FilterActionKind; label: string }[] = [
  { value: "move", label: "Move to folder" },
  { value: "copy", label: "Copy to folder" },
  { value: "tag", label: "Tag" },
  { value: "star", label: "Star" },
  { value: "mark_read", label: "Mark read" },
  { value: "mark_unread", label: "Mark unread" },
  { value: "mark_junk", label: "Mark as junk" },
  { value: "delete_to_trash", label: "Delete to Trash" },
  { value: "forward", label: "Forward to" },
  { value: "stop", label: "Stop further filters" },
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
function opsFor(field: FilterConditionField): { value: FilterConditionOp; label: string }[] {
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
    aria-label="Filter editor"
    @submit.prevent="save"
  >
    <label class="field">
      <span>Name</span>
      <input
        v-model="draft.name"
        type="text"
        aria-label="Filter name"
        placeholder="e.g. File invoices"
      >
    </label>

    <label class="field">
      <span>Match</span>
      <select
        v-model="draft.matchType"
        aria-label="Match type"
      >
        <option value="all">
          all conditions
        </option>
        <option value="any">
          any condition
        </option>
      </select>
    </label>

    <fieldset class="rows">
      <legend>Conditions</legend>
      <div
        v-for="(row, index) in draft.conditions"
        :key="index"
        class="row"
      >
        <select
          v-model="row.field"
          aria-label="Condition field"
          @change="onFieldChange(row)"
        >
          <option
            v-for="f in FIELDS"
            :key="f.value"
            :value="f.value"
          >
            {{ f.label }}
          </option>
        </select>
        <select
          v-model="row.op"
          aria-label="Condition operator"
        >
          <option
            v-for="o in opsFor(row.field)"
            :key="o.value"
            :value="o.value"
          >
            {{ o.label }}
          </option>
        </select>
        <select
          v-if="row.field === 'has_attachment'"
          v-model="row.value"
          aria-label="Condition value"
        >
          <option value="true">
            yes
          </option>
          <option value="false">
            no
          </option>
        </select>
        <input
          v-else
          v-model="row.value"
          :type="row.field === 'date' ? 'date' : row.field === 'size' ? 'number' : 'text'"
          aria-label="Condition value"
        >
        <button
          type="button"
          class="icon"
          aria-label="Remove condition"
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
        + Add condition
      </button>
    </fieldset>

    <fieldset class="rows">
      <legend>Actions</legend>
      <div
        v-for="(row, index) in draft.actions"
        :key="index"
        class="row"
      >
        <select
          v-model="row.kind"
          aria-label="Action"
        >
          <option
            v-for="a in ACTIONS"
            :key="a.value"
            :value="a.value"
          >
            {{ a.label }}
          </option>
        </select>
        <select
          v-if="paramKind(row.kind) === 'folder'"
          v-model="row.param"
          aria-label="Target folder"
        >
          <option
            value=""
            disabled
          >
            Choose folder…
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
          aria-label="Tag name"
          placeholder="Tag name"
        >
        <input
          v-else-if="paramKind(row.kind) === 'email'"
          v-model="row.param"
          type="email"
          aria-label="Forward address"
          placeholder="name@example.com"
        >
        <button
          type="button"
          class="icon"
          aria-label="Remove action"
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
        + Add action
      </button>
    </fieldset>

    <label class="check">
      <input
        v-model="draft.stopOnMatch"
        type="checkbox"
      >
      <span>Stop evaluating later filters when this one matches</span>
    </label>

    <div class="buttons">
      <button
        type="submit"
        class="action"
        :disabled="draft.name.trim() === ''"
      >
        Save filter
      </button>
      <button
        type="button"
        class="action"
        @click="emit('cancel')"
      >
        Cancel
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
