<script setup lang="ts">
/**
 * Command palette (Ctrl/Cmd+K): a modal combobox listing every registered
 * command, fuzzy-filtered as you type, navigable with the arrow keys, and run
 * with Enter or a click. Running a command closes the palette and invokes it.
 *
 * It reads the shared registry through {@link useCommandRegistry}, so it needs
 * no props beyond the `close` event: whatever a feature registered (with its
 * shortcut and enabled state) shows up automatically. Focus stays on the input;
 * the active option is tracked with `aria-activedescendant` per the ARIA
 * combobox pattern.
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";

import { formatShortcutHint } from "../../commands/display";
import { useCommandRegistry } from "../../composables/useCommandRegistry";
import type { Command } from "../../types/command";
import { filterCommands, highlightSegments } from "./fuzzy";

const emit = defineEmits<{ close: [] }>();

const registry = useCommandRegistry();
const query = ref("");
const activeIndex = ref(0);
const input = ref<HTMLInputElement | null>(null);

const LIST_ID = "command-palette-listbox";

// The listbox popup is always rendered — it shows an explicit "no matching
// commands" row when the query matches nothing — so the combobox is always
// expanded while the palette is open. aria-expanded must track the popup's
// visibility, not the result count (a visible empty popup with expanded=false
// misreports state to assistive tech).
const popupVisible = true;

const results = computed(() => filterCommands(registry.commands.value, query.value));

// Any change to the filtered set resets the highlight to the top match.
watch(results, () => {
  activeIndex.value = 0;
});

onMounted(() => {
  void nextTick(() => input.value?.focus());
});

function optionId(index: number): string {
  return `command-palette-option-${index}`;
}

const activeOptionId = computed(() =>
  results.value.length > 0 ? optionId(activeIndex.value) : undefined,
);

function isEnabled(command: Command): boolean {
  return command.isEnabled?.() ?? true;
}

function segments(command: Command, ranges: [number, number][]) {
  return highlightSegments(command.title, ranges);
}

function move(delta: number): void {
  const count = results.value.length;
  if (count === 0) return;
  activeIndex.value = (activeIndex.value + delta + count) % count;
  scrollActiveIntoView();
}

function setActive(index: number): void {
  if (results.value.length === 0) return;
  activeIndex.value = Math.max(0, Math.min(results.value.length - 1, index));
  scrollActiveIntoView();
}

function scrollActiveIntoView(): void {
  void nextTick(() => {
    document
      .getElementById(optionId(activeIndex.value))
      ?.scrollIntoView({ block: "nearest" });
  });
}

/** Runs the command at `index` (if enabled), closing the palette first. */
function run(index: number): void {
  const entry = results.value[index];
  if (entry === undefined || !isEnabled(entry.command)) return;
  emit("close");
  void entry.command.run();
}

function onKeydown(event: KeyboardEvent): void {
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      move(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      move(-1);
      break;
    case "Home":
      event.preventDefault();
      setActive(0);
      break;
    case "End":
      event.preventDefault();
      setActive(results.value.length - 1);
      break;
    case "Enter":
      event.preventDefault();
      run(activeIndex.value);
      break;
    case "Escape":
      event.preventDefault();
      emit("close");
      break;
    case "Tab":
      // Trap focus: the input is the palette's only focusable control, so keep
      // focus on it rather than letting Tab escape to the background (which the
      // palette's deliberate exclusion from isModalOpen would otherwise expose).
      event.preventDefault();
      input.value?.focus();
      break;
    default:
      break;
  }
}
</script>

<template>
  <div
    class="overlay"
    @click.self="emit('close')"
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      @keydown="onKeydown"
    >
      <input
        ref="input"
        v-model="query"
        type="text"
        class="input"
        role="combobox"
        aria-label="Search commands"
        placeholder="Type a command…"
        autocomplete="off"
        aria-autocomplete="list"
        :aria-expanded="popupVisible"
        :aria-controls="LIST_ID"
        :aria-activedescendant="activeOptionId"
      >
      <ul
        :id="LIST_ID"
        class="list"
        role="listbox"
        aria-label="Commands"
      >
        <li
          v-for="(entry, index) in results"
          :id="optionId(index)"
          :key="entry.command.id"
          class="option"
          :class="{ active: index === activeIndex, disabled: !isEnabled(entry.command) }"
          role="option"
          :aria-selected="index === activeIndex"
          :aria-disabled="!isEnabled(entry.command)"
          @click="run(index)"
          @mousemove="setActive(index)"
        >
          <span class="title">
            <template
              v-for="(segment, si) in segments(entry.command, entry.ranges)"
              :key="si"
            >
              <mark
                v-if="segment.match"
                class="hit"
              >{{ segment.text }}</mark>
              <template v-else>{{ segment.text }}</template>
            </template>
          </span>
          <kbd
            v-if="formatShortcutHint(entry.command.shortcut)"
            class="shortcut"
          >
            {{ formatShortcutHint(entry.command.shortcut) }}
          </kbd>
        </li>
        <li
          v-if="results.length === 0"
          class="empty"
          role="status"
        >
          No matching commands.
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  background: rgb(0 0 0 / 40%);
}

.palette {
  width: 100%;
  max-width: 540px;
  max-height: 60vh;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg, 0 10px 40px rgb(0 0 0 / 30%));
  overflow: hidden;
}

.input {
  font: inherit;
  font-size: var(--text-md);
  color: var(--color-text);
  background: var(--color-bg);
  border: none;
  border-bottom: 1px solid var(--color-border);
  padding: var(--space-3) var(--space-4);
}

.input:focus-visible {
  outline: none;
}

.list {
  list-style: none;
  margin: 0;
  padding: var(--space-1);
  overflow-y: auto;
}

.option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.option.active {
  background: var(--color-bg-selected);
}

.option.disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hit {
  background: transparent;
  color: var(--color-accent);
  font-weight: var(--weight-semibold);
}

.shortcut {
  flex: none;
  font-size: var(--text-xs);
  font-family: inherit;
  color: var(--color-text-secondary);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 0 var(--space-2);
}

.empty {
  padding: var(--space-4);
  text-align: center;
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}
</style>
