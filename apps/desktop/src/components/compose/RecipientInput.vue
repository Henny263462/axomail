<script setup lang="ts">
/**
 * A recipient field with contact autocomplete: a single comma-separated address
 * string (the `v-model`) with an ARIA combobox typeahead over the address book.
 *
 * The field owns the whole comma-separated value; the dropdown searches only the
 * token after the last comma. Selecting a suggestion replaces that token with
 * the address (or, for a group, its comma-joined members) and appends a
 * separator so the user can keep typing. Suggestions come from the `search_contacts`
 * backend via {@link searchRecipients}; the field degrades to a plain input in
 * the browser preview (no results). Keyboard: ArrowUp/Down move the active
 * option, Enter selects it, Escape closes.
 */
import { nextTick, onBeforeUnmount, ref, useId } from "vue";

import { searchRecipients } from "../../api/contacts";
import type { RecipientSuggestionDto } from "../../types/mail";

const props = withDefaults(
  defineProps<{
    /** Field id (labels the input). */
    fieldId: string;
    /** Visible label text (also the input's accessible name). */
    label: string;
    /** Debounce before querying, in ms (0 in tests for determinism). */
    debounceMs?: number;
  }>(),
  { debounceMs: 140 },
);

const model = defineModel<string>({ default: "" });

const input = ref<HTMLInputElement | null>(null);
const suggestions = ref<RecipientSuggestionDto[]>([]);
const open = ref(false);
const activeIndex = ref(-1);
const listId = useId();

let timer: ReturnType<typeof setTimeout> | null = null;
let queryToken = 0; // Monotonic guard so a stale async result can't win.

defineExpose({
  focus: (): void => input.value?.focus(),
});

onBeforeUnmount(() => {
  if (timer !== null) clearTimeout(timer);
});

/** The address currently being typed: the text after the last comma. */
function currentToken(): string {
  const value = model.value;
  const comma = value.lastIndexOf(",");
  return (comma === -1 ? value : value.slice(comma + 1)).trim();
}

function onInput(): void {
  const token = currentToken();
  if (timer !== null) clearTimeout(timer);
  if (token.length === 0) {
    close();
    return;
  }
  timer = setTimeout(() => void runSearch(token), props.debounceMs);
}

async function runSearch(token: string): Promise<void> {
  const seq = ++queryToken;
  const results = await searchRecipients(token, 8);
  if (seq !== queryToken) return; // A newer query superseded this one.
  suggestions.value = results ?? [];
  open.value = suggestions.value.length > 0;
  activeIndex.value = suggestions.value.length > 0 ? 0 : -1;
}

/** Replaces the token being typed with the chosen address(es). */
function choose(suggestion: RecipientSuggestionDto): void {
  const value = model.value;
  const comma = value.lastIndexOf(",");
  const head = comma === -1 ? "" : value.slice(0, comma + 1);
  const prefix = head.trim().length > 0 ? `${head.trimEnd()} ` : "";
  model.value = `${prefix}${suggestion.value}, `;
  close();
  void nextTick(() => input.value?.focus());
}

function close(): void {
  open.value = false;
  suggestions.value = [];
  activeIndex.value = -1;
  queryToken += 1;
}

function onKeydown(event: KeyboardEvent): void {
  if (!open.value) return;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    activeIndex.value = (activeIndex.value + 1) % suggestions.value.length;
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    activeIndex.value =
      (activeIndex.value - 1 + suggestions.value.length) % suggestions.value.length;
  } else if (event.key === "Enter") {
    const active = suggestions.value[activeIndex.value];
    if (active) {
      event.preventDefault();
      choose(active);
    }
  } else if (event.key === "Escape") {
    event.preventDefault();
    close();
  }
}

function optionId(index: number): string {
  return `${listId}-opt-${index}`;
}
</script>

<template>
  <div class="field">
    <label :for="fieldId">{{ label }}</label>
    <div class="combo">
      <input
        :id="fieldId"
        ref="input"
        v-model="model"
        type="text"
        autocomplete="off"
        role="combobox"
        aria-autocomplete="list"
        :aria-expanded="open"
        :aria-controls="listId"
        :aria-activedescendant="
          open && activeIndex >= 0 ? optionId(activeIndex) : undefined
        "
        placeholder="Comma-separated addresses"
        @input="onInput"
        @keydown="onKeydown"
        @blur="close"
      >
      <ul
        v-if="open"
        :id="listId"
        class="listbox"
        role="listbox"
        :aria-label="`${label} suggestions`"
      >
        <li
          v-for="(item, index) in suggestions"
          :id="optionId(index)"
          :key="`${item.kind}:${item.value}`"
          class="option"
          :class="{ active: index === activeIndex }"
          role="option"
          :aria-selected="index === activeIndex"
          @mousedown.prevent="choose(item)"
          @mouseenter="activeIndex = index"
        >
          <span class="option-label">
            <span
              v-if="item.kind === 'group'"
              class="badge"
              aria-hidden="true"
            >group</span>
            {{ item.label }}
          </span>
          <span class="option-detail">{{ item.detail }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.combo {
  position: relative;
}

.combo input {
  width: 100%;
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.combo input:focus-visible {
  border-color: var(--color-accent);
}

.listbox {
  position: absolute;
  z-index: 20;
  top: calc(100% + 2px);
  left: 0;
  right: 0;
  margin: 0;
  padding: var(--space-1);
  list-style: none;
  max-height: 14rem;
  overflow-y: auto;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md, 0 4px 12px rgb(0 0 0 / 0.15));
}

.option {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.option.active {
  background: var(--color-bg-hover);
}

.option-label {
  font-size: var(--text-sm);
  color: var(--color-text);
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.option-detail {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.badge {
  font-size: var(--text-2xs, 0.65rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-accent);
  border: 1px solid var(--color-accent);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-1);
}
</style>
