<script setup lang="ts">
/**
 * Folder properties: the per-mailbox offline policy, an immediate "download
 * now", and a cache cleanup. Opened from the message-list region's own
 * affordance (kept separate from the sidebar's folder context menu to avoid
 * colliding with the folder-tree track). Self-contained: it loads and saves the
 * policy through `api/offline` and reports outcomes via an aria-live region.
 */
import { nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import {
  cleanupOfflineCache,
  downloadMailboxNow,
  getMailboxOfflinePolicy,
  setMailboxOfflinePolicy,
} from "../../api/offline";
import type { OfflineMode } from "../../types/mail";
import FolderRetentionSection from "./FolderRetentionSection.vue";

const props = withDefaults(
  defineProps<{
    mailboxId: number;
    mailboxName?: string;
  }>(),
  { mailboxName: "" },
);

const emit = defineEmits<{
  close: [];
}>();

const { t } = useI18n({ useScope: "global" });

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const mode = ref<OfflineMode>("recent");
const recentDays = ref(30);
const busy = ref(false);
const status = ref<string | null>(null);

const root = ref<HTMLElement | null>(null);

async function load(): Promise<void> {
  const policy = await getMailboxOfflinePolicy(props.mailboxId);
  if (policy === null) {
    state.value = "unavailable";
    return;
  }
  mode.value = policy.mode;
  recentDays.value = policy.recentDays;
  state.value = "ready";
}

async function onSave(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  status.value = null;
  try {
    const days = Math.max(1, Math.floor(recentDays.value) || 1);
    await setMailboxOfflinePolicy(props.mailboxId, mode.value, days);
    status.value = t("offline.props.saved");
  } catch {
    status.value = t("offline.props.saveFailed");
  } finally {
    busy.value = false;
  }
}

async function onDownloadNow(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  status.value = null;
  try {
    await downloadMailboxNow(props.mailboxId);
    status.value = t("offline.props.downloadStarted");
  } catch {
    status.value = t("offline.props.downloadFailed");
  } finally {
    busy.value = false;
  }
}

async function onCleanup(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  status.value = null;
  const purged = await cleanupOfflineCache(props.mailboxId);
  busy.value = false;
  status.value =
    purged === null
      ? t("offline.props.cleanupUnavailable")
      : t("offline.props.cleanupDone", { count: purged }, purged);
}

function close(): void {
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>(
      "button:not([disabled]), select, input",
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

onMounted(async () => {
  await load();
  await nextTick();
  focusables()[0]?.focus();
});
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
      aria-labelledby="folder-props-title"
    >
      <header class="head">
        <h2
          id="folder-props-title"
          class="title"
        >
          {{ t("offline.props.title") }}{{ mailboxName ? t("offline.props.titleSuffix", { name: mailboxName }) : "" }}
        </h2>
      </header>

      <h3 class="section-title">
        {{ t("offline.props.availability") }}
      </h3>
      <p
        v-if="state === 'loading'"
        class="note"
        role="status"
      >
        {{ t("offline.props.loading") }}
      </p>
      <p
        v-else-if="state === 'unavailable'"
        class="note"
        role="status"
      >
        {{ t("offline.props.unavailable") }}
      </p>

      <div
        v-else
        class="body"
      >
        <fieldset class="modes">
          <legend class="legend">
            {{ t("offline.props.keepOffline") }}
          </legend>
          <label class="choice">
            <input
              v-model="mode"
              type="radio"
              value="none"
              name="offline-mode"
            >
            <span>{{ t("offline.props.modeNothing") }}</span>
          </label>
          <label class="choice">
            <input
              v-model="mode"
              type="radio"
              value="recent"
              name="offline-mode"
            >
            <span>{{ t("offline.props.modeRecent") }}</span>
          </label>
          <label class="choice">
            <input
              v-model="mode"
              type="radio"
              value="all"
              name="offline-mode"
            >
            <span>{{ t("offline.props.modeAll") }}</span>
          </label>
        </fieldset>

        <label
          v-if="mode === 'recent'"
          class="days"
        >
          <span>{{ t("offline.props.daysToKeep") }}</span>
          <input
            v-model.number="recentDays"
            type="number"
            min="1"
            :aria-label="t('offline.props.daysToKeepAria')"
          >
        </label>

        <p
          v-if="status !== null"
          class="status"
          role="status"
          aria-live="polite"
        >
          {{ status }}
        </p>
      </div>

      <FolderRetentionSection :mailbox-id="mailboxId" />

      <div class="actions">
        <button
          type="button"
          class="button"
          :disabled="busy || state !== 'ready'"
          @click="onDownloadNow"
        >
          {{ t("offline.props.downloadNow") }}
        </button>
        <button
          type="button"
          class="button"
          :disabled="busy || state !== 'ready'"
          @click="onCleanup"
        >
          {{ t("offline.props.cleanupCache") }}
        </button>
        <span class="spacer" />
        <button
          type="button"
          class="button"
          @click="close"
        >
          {{ t("common.close") }}
        </button>
        <button
          type="button"
          class="button primary"
          :disabled="busy || state !== 'ready'"
          @click="onSave"
        >
          {{ t("common.save") }}
        </button>
      </div>
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
  max-width: 440px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.head {
  margin-bottom: var(--space-4);
}

.title {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.note {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.body {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.section-title {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.modes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  border: none;
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin-bottom: var(--space-1);
}

.choice {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}

.days {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.days input {
  width: 5rem;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}

.status {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: var(--space-5);
}

.spacer {
  flex: 1;
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
  transition: background var(--transition-fast);
}

.button:hover {
  background: var(--color-bg-hover);
}

.button:disabled {
  opacity: 0.6;
  cursor: default;
}

.button.primary:not(:disabled) {
  color: var(--color-text-inverse);
  background: var(--color-accent);
  border-color: var(--color-accent);
}
</style>
