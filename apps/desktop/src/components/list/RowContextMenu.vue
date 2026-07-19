<script setup lang="ts">
/**
 * Right-click context menu for a message row (or the current multi-selection):
 * the same relocation actions as the reading-pane bar. Positioned at the cursor
 * via a Teleport, with a full-screen backdrop that closes it, Escape to cancel,
 * and arrow-key roving over the menu items.
 */
import { onMounted, ref } from "vue";

import type { BulkAction } from "../../composables/useMailboxActions";

defineProps<{
  x: number;
  y: number;
}>();

const emit = defineEmits<{
  action: [action: BulkAction];
  close: [];
}>();

const root = ref<HTMLElement | null>(null);

const items: { action: BulkAction; label: string; danger?: boolean }[] = [
  { action: "archive", label: "Archive" },
  { action: "move", label: "Move to…" },
  { action: "copy", label: "Copy to…" },
  { action: "markRead", label: "Mark read" },
  { action: "delete", label: "Delete", danger: true },
];

onMounted(() => {
  root.value?.querySelector<HTMLButtonElement>(".item")?.focus();
});

function choose(action: BulkAction): void {
  emit("action", action);
  emit("close");
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    emit("close");
    return;
  }
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  const buttons = [...(root.value?.querySelectorAll<HTMLButtonElement>(".item") ?? [])];
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
  const delta = event.key === "ArrowDown" ? 1 : -1;
  const next = (current + delta + buttons.length) % buttons.length;
  buttons[next]?.focus();
}
</script>

<template>
  <Teleport to="body">
    <div
      class="backdrop"
      @click="emit('close')"
      @contextmenu.prevent="emit('close')"
    />
    <div
      ref="root"
      class="menu"
      role="menu"
      aria-label="Message actions"
      :style="{ left: x + 'px', top: y + 'px' }"
      @keydown="onKeydown"
    >
      <button
        v-for="item in items"
        :key="item.action"
        type="button"
        role="menuitem"
        class="item"
        :class="{ danger: item.danger }"
        @click="choose(item.action)"
      >
        {{ item.label }}
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 150;
}

.menu {
  position: fixed;
  z-index: 151;
  min-width: 160px;
  display: flex;
  flex-direction: column;
  padding: var(--space-1);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated, var(--color-bg));
  box-shadow: var(--shadow-md, 0 4px 12px rgb(0 0 0 / 20%));
}

.item {
  text-align: left;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-3);
  cursor: pointer;
}

.item:hover,
.item:focus-visible {
  background: var(--color-bg-hover);
  outline: none;
}

.item.danger {
  color: var(--color-danger);
}
</style>
