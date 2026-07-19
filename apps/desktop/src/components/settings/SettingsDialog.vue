<script setup lang="ts">
/**
 * Settings dialog shell.
 *
 * Owns the modal overlay, the focus trap, and the section switcher; each
 * section's fields and behaviour live in its own component (a lightweight
 * stand-in for the planned settings-section registry). "General" holds the
 * application-wide settings; "Accounts" manages the configured accounts.
 */
import { ref } from "vue";

import type { AccountSummary } from "../../types/mail";
import AccountsSection from "./AccountsSection.vue";
import GeneralSettingsSection from "./GeneralSettingsSection.vue";

defineProps<{
  /** Accounts, forwarded to the General section for its POP3 labels. */
  accounts: AccountSummary[];
}>();

const emit = defineEmits<{
  close: [];
}>();

type Section = "general" | "accounts";
const section = ref<Section>("general");

const sections: { id: Section; label: string }[] = [
  { id: "general", label: "General" },
  { id: "accounts", label: "Accounts" },
];

const root = ref<HTMLElement | null>(null);

function close(): void {
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), select:not([disabled])",
    ),
  );
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusables();
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
    >
      <div class="header">
        <h2
          id="settings-title"
          class="title"
        >
          Settings
        </h2>
        <button
          type="button"
          class="close"
          aria-label="Close settings"
          @click="close"
        >
          ×
        </button>
      </div>

      <div
        class="sections"
        role="tablist"
        aria-label="Settings sections"
      >
        <button
          v-for="item in sections"
          :key="item.id"
          type="button"
          role="tab"
          class="tab"
          :class="{ active: section === item.id }"
          :aria-selected="section === item.id"
          @click="section = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <GeneralSettingsSection
        v-if="section === 'general'"
        :accounts="accounts"
        @close="close"
      />
      <AccountsSection v-else-if="section === 'accounts'" />
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 460px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-3);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.close {
  font-size: var(--text-lg);
  line-height: 1;
  color: var(--color-text-secondary);
  background: none;
  border: none;
  padding: var(--space-1);
  cursor: pointer;
}

.close:hover {
  color: var(--color-text);
}

.sections {
  display: flex;
  gap: var(--space-1);
  margin-bottom: var(--space-4);
  border-bottom: 1px solid var(--color-border);
}

.tab {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
}

.tab:hover {
  color: var(--color-text);
}

.tab.active {
  color: var(--color-text);
  border-bottom-color: var(--color-accent);
}
</style>
