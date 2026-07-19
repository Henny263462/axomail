<script setup lang="ts">
/**
 * Tag manager settings section: per account, list tags and rename, recolor,
 * or delete them (with an "are you sure" step when a tag is in use), and
 * create new tags. Autonomous — it persists through the tag api and reports
 * outcomes via the shared {@link SettingsSectionContext} aria-live regions.
 */
import { ref, watch } from "vue";

import { createTag, deleteTag, listTags, updateTag } from "../../api/tags";
import { isTauri } from "../../lib/tauri";
import type { TagDto } from "../../types/mail";
import { messageFrom, type SettingsSectionContext } from "./context";

/** Fallback swatch shown for a colorless tag before a color is chosen. */
const DEFAULT_SWATCH = "#3c78d8";

interface EditRow {
  id: number;
  keyword: string;
  displayName: string;
  color: string | null;
  usageCount: number;
}

const props = defineProps<{ context: SettingsSectionContext }>();

const accountId = ref<number | null>(props.context.accounts[0]?.id ?? null);
const rows = ref<EditRow[]>([]);
const newName = ref("");
const newColor = ref<string | null>(DEFAULT_SWATCH);
const pendingDelete = ref<number | null>(null);

watch(accountId, load, { immediate: true });

async function load(): Promise<void> {
  pendingDelete.value = null;
  if (accountId.value === null) {
    rows.value = [];
    return;
  }
  const tags = await listTags(accountId.value);
  rows.value = (tags ?? []).map(toRow);
}

function toRow(tag: TagDto): EditRow {
  return {
    id: tag.id,
    keyword: tag.keyword,
    displayName: tag.displayName,
    color: tag.color,
    usageCount: tag.usageCount,
  };
}

function guardTauri(): boolean {
  if (isTauri()) return true;
  props.context.reportError(
    "Managing tags needs the desktop app; it is unavailable in the browser preview.",
  );
  return false;
}

async function saveRow(row: EditRow): Promise<void> {
  props.context.reportStatus("");
  if (row.displayName.trim() === "") {
    props.context.reportError("A tag name must not be empty.");
    return;
  }
  if (!guardTauri()) return;
  try {
    await updateTag(row.id, row.displayName.trim(), row.color);
    props.context.reportError("");
    props.context.reportStatus(`Saved “${row.displayName.trim()}”.`);
    // Name/color changed: refresh the sidebar Tags group.
    props.context.notifyTagsChanged();
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

function requestDelete(row: EditRow): void {
  if (row.usageCount > 0) {
    pendingDelete.value = row.id;
    return;
  }
  void removeRow(row);
}

async function removeRow(row: EditRow): Promise<void> {
  pendingDelete.value = null;
  props.context.reportStatus("");
  if (!guardTauri()) return;
  try {
    await deleteTag(row.id);
    rows.value = rows.value.filter((candidate) => candidate.id !== row.id);
    props.context.reportError("");
    props.context.reportStatus(`Deleted “${row.displayName}”.`);
    props.context.notifyTagsChanged();
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

async function addTag(): Promise<void> {
  props.context.reportStatus("");
  const name = newName.value.trim();
  if (name === "" || accountId.value === null) {
    props.context.reportError("A tag name must not be empty.");
    return;
  }
  if (!guardTauri()) return;
  try {
    const tag = await createTag(accountId.value, name, newColor.value);
    if (tag !== null) rows.value.push(toRow(tag));
    newName.value = "";
    props.context.reportError("");
    props.context.reportStatus(`Created “${name}”.`);
    props.context.notifyTagsChanged();
  } catch (error) {
    props.context.reportError(messageFrom(error));
  }
}

function setColor(row: EditRow, value: string): void {
  row.color = value;
}
</script>

<template>
  <fieldset class="group">
    <legend class="legend">
      Tags
    </legend>

    <label
      v-if="context.accounts.length > 1"
      class="account-select"
    >
      <span>Account</span>
      <select v-model="accountId">
        <option
          v-for="account in context.accounts"
          :key="account.id"
          :value="account.id"
        >
          {{ account.displayName }}
        </option>
      </select>
    </label>

    <p
      v-if="rows.length === 0"
      class="empty"
    >
      No tags yet. Create one below.
    </p>

    <ul
      v-else
      class="tag-list"
    >
      <li
        v-for="row in rows"
        :key="row.id"
        class="tag-row"
      >
        <input
          type="color"
          class="swatch"
          :value="row.color ?? DEFAULT_SWATCH"
          :aria-label="`Color for ${row.displayName}`"
          @input="setColor(row, ($event.target as HTMLInputElement).value)"
        >
        <button
          v-if="row.color !== null"
          type="button"
          class="link"
          @click="row.color = null"
        >
          Clear
        </button>
        <input
          v-model="row.displayName"
          type="text"
          class="name"
          :aria-label="`Name for ${row.keyword}`"
        >
        <button
          type="button"
          class="action"
          @click="saveRow(row)"
        >
          Save
        </button>
        <template v-if="pendingDelete === row.id">
          <span class="confirm-text">On {{ row.usageCount }} message(s). Delete?</span>
          <button
            type="button"
            class="danger"
            @click="removeRow(row)"
          >
            Confirm
          </button>
          <button
            type="button"
            class="action"
            @click="pendingDelete = null"
          >
            Cancel
          </button>
        </template>
        <button
          v-else
          type="button"
          class="danger"
          @click="requestDelete(row)"
        >
          Delete
        </button>
      </li>
    </ul>

    <form
      class="create"
      @submit.prevent="addTag"
    >
      <input
        type="color"
        class="swatch"
        :value="newColor ?? DEFAULT_SWATCH"
        aria-label="New tag color"
        @input="newColor = ($event.target as HTMLInputElement).value"
      >
      <input
        v-model="newName"
        type="text"
        class="name"
        placeholder="New tag name"
        aria-label="New tag name"
      >
      <button
        type="submit"
        class="action"
        :disabled="newName.trim() === ''"
      >
        Add tag
      </button>
    </form>
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

.account-select {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
}

.empty {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.tag-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.tag-row,
.create {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.swatch {
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: none;
}

.name {
  flex: 1;
  min-width: 120px;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.action,
.danger,
.link {
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  background: var(--color-bg);
}

.action {
  border: 1px solid var(--color-border-strong);
  color: var(--color-accent);
}

.danger {
  border: 1px solid var(--color-danger);
  color: var(--color-danger);
}

.link {
  border: none;
  color: var(--color-text-tertiary);
  text-decoration: underline;
}

.confirm-text {
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
