<script setup lang="ts">
/**
 * The per-message tag picker: a keyboard-navigable list of the account's tags
 * (toggle to apply/remove) with an inline "create new tag" field. Purely a
 * controlled view — it owns no data, emitting `toggle` and `create` for the
 * host ({@link MessageTagArea}) to persist, and `close` on Escape or blur-out.
 */
import { nextTick, onMounted, ref } from "vue";

import type { MessageTagOption } from "../../types/mail";

defineProps<{
  options: MessageTagOption[];
  /** Disables inputs while a mutation is in flight. */
  busy: boolean;
}>();

const emit = defineEmits<{
  toggle: [tagId: number, applied: boolean];
  create: [name: string];
  close: [];
}>();

const newName = ref("");
const root = ref<HTMLElement | null>(null);

onMounted(async () => {
  await nextTick();
  // Focus the first control (the first tag's checkbox, or the create field
  // when there are no tags yet) so the picker is usable from the keyboard.
  root.value?.querySelector<HTMLElement>("input")?.focus();
});

function onToggle(option: MessageTagOption, event: Event): void {
  emit("toggle", option.id, (event.target as HTMLInputElement).checked);
}

function onCreate(): void {
  const name = newName.value.trim();
  if (name === "") return;
  emit("create", name);
  newName.value = "";
}

function dotStyle(color: string | null): Record<string, string> {
  return color === null
    ? { border: "1px solid var(--color-border-strong)" }
    : { backgroundColor: color };
}
</script>

<template>
  <div
    ref="root"
    class="picker"
    role="group"
    aria-label="Tag this message"
    @keydown.esc.stop.prevent="emit('close')"
  >
    <p
      v-if="options.length === 0"
      class="empty"
    >
      No tags yet. Create one below.
    </p>
    <ul
      v-else
      class="options"
    >
      <li
        v-for="option in options"
        :key="option.id"
      >
        <label class="option">
          <!-- Never disabled while busy: disabling the focused checkbox blurs
               it, which fires the host's focus-out and snaps the picker shut.
               Toggles are idempotent and the host guards stale results. -->
          <input
            type="checkbox"
            :checked="option.applied"
            @change="onToggle(option, $event)"
          >
          <span
            class="dot"
            :style="dotStyle(option.color)"
            aria-hidden="true"
          />
          <span class="name">{{ option.displayName }}</span>
        </label>
      </li>
    </ul>

    <form
      class="create"
      @submit.prevent="onCreate"
    >
      <input
        v-model="newName"
        type="text"
        class="new-name"
        placeholder="New tag name"
        aria-label="New tag name"
        :disabled="busy"
      >
      <button
        type="submit"
        class="add"
        :disabled="busy || newName.trim() === ''"
      >
        Add
      </button>
    </form>
  </div>
</template>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 220px;
  padding: var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  box-shadow: var(--shadow-md, 0 4px 16px rgb(0 0 0 / 0.12));
}

.empty {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.options {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 240px;
  overflow-y: auto;
}

.option {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-1);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.option:hover {
  background: var(--color-bg-hover);
}

.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.create {
  display: flex;
  gap: var(--space-2);
  border-top: 1px solid var(--color-border);
  padding-top: var(--space-2);
}

.new-name {
  flex: 1;
  min-width: 0;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.add {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--color-accent);
  background: var(--color-bg);
}

.add:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}
</style>
