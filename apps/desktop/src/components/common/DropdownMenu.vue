<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from "vue";

import AppIcon from "./AppIcon.vue";
import type { MenuItem } from "./menu";

const props = withDefaults(
  defineProps<{
    items: MenuItem[];
    label: string;
    align?: "start" | "end";
    disabled?: boolean;
  }>(),
  { align: "start", disabled: false },
);

const emit = defineEmits<{ select: [id: string] }>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
const menu = ref<HTMLElement | null>(null);

function onDocumentPointer(event: PointerEvent): void {
  if (root.value && !root.value.contains(event.target as Node)) close();
}

function enabledItems(): HTMLElement[] {
  return Array.from(menu.value?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []);
}

async function show(): Promise<void> {
  if (props.disabled) return;
  open.value = true;
  document.addEventListener("pointerdown", onDocumentPointer, true);
  await nextTick();
  enabledItems()[0]?.focus();
}

function close(): void {
  open.value = false;
  document.removeEventListener("pointerdown", onDocumentPointer, true);
}

function toggle(): void {
  if (open.value) close();
  else void show();
}

function choose(item: MenuItem): void {
  if (item.disabled) return;
  close();
  emit("select", item.id);
}

function onMenuKey(event: KeyboardEvent): void {
  const items = enabledItems();
  const index = items.indexOf(document.activeElement as HTMLElement);
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    close();
    root.value?.querySelector<HTMLElement>(".trigger")?.focus();
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    items[(index + step + items.length) % items.length]?.focus();
  } else if (event.key === "Tab") {
    close();
  }
}

onBeforeUnmount(close);
</script>

<template>
  <div
    ref="root"
    class="dropdown"
  >
    <button
      type="button"
      class="trigger"
      :class="{ open }"
      :disabled="disabled"
      aria-haspopup="menu"
      :aria-expanded="open"
      :aria-label="label"
      :title="label"
      @click="toggle"
    >
      <slot />
    </button>
    <Transition name="menu-pop">
      <div
        v-if="open"
        ref="menu"
        class="menu"
        :class="align"
        role="menu"
        :aria-label="label"
        @keydown="onMenuKey"
      >
        <template
          v-for="item in items"
          :key="item.id"
        >
          <div
            v-if="item.separatorBefore"
            class="separator"
            role="separator"
          />
          <button
            type="button"
            role="menuitem"
            class="item"
            :class="{ danger: item.danger }"
            :disabled="item.disabled"
            @click="choose(item)"
          >
            <AppIcon
              v-if="item.icon"
              :name="item.icon"
              :size="15"
            />
            <span>{{ item.label }}</span>
          </button>
        </template>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.dropdown {
  position: relative;
  display: inline-flex;
}

.trigger {
  display: inline-flex;
  align-items: center;
}

.menu {
  position: absolute;
  top: calc(100% + 4px);
  z-index: 150;
  min-width: 210px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  transform-origin: top left;
}

.menu.start {
  left: 0;
}

.menu.end {
  right: 0;
  transform-origin: top right;
}

.item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: 6px var(--space-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--color-text);
  text-align: left;
  white-space: nowrap;
}

.item:hover:not(:disabled),
.item:focus-visible {
  background: var(--color-bg-hover);
  box-shadow: none;
}

.item:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.item.danger:not(:disabled) {
  color: var(--color-danger);
}

.separator {
  height: 1px;
  margin: 4px 2px;
  background: var(--color-border);
}

.menu-pop-enter-active {
  transition:
    opacity var(--duration-fast) ease,
    transform var(--duration-base) var(--ease-spring);
}

.menu-pop-leave-active {
  transition:
    opacity var(--duration-fast) ease,
    transform var(--duration-fast) ease;
}

.menu-pop-enter-from,
.menu-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.96);
}
</style>
