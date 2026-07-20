<script setup lang="ts">
import { onMounted } from "vue";
import { useI18n } from "vue-i18n";

import { useAppVersion } from "../../composables/useAppVersion";
import { usePushStatus } from "../../composables/usePushStatus";
import { useWorkOffline } from "../../composables/useWorkOffline";
import { isTauri } from "../../lib/tauri";
import LayoutSwitcher from "./LayoutSwitcher.vue";

const props = withDefaults(
  defineProps<{
    /** Redacted sync failure to surface, or null when there is none. */
    error?: string | null;
    /** True while at least one sync job is queued or running. */
    syncing?: boolean;
    /** Number of jobs currently queued or running. */
    syncCount?: number;
  }>(),
  { error: null, syncing: false, syncCount: 0 },
);

const emit = defineEmits<{
  dismissError: [];
  openSettings: [];
  openActivity: [];
  cancelSync: [];
}>();

const { t } = useI18n({ useScope: "global" });
const { version } = useAppVersion();
const browserPreview = !isTauri();

// Push (IMAP IDLE) status: a small badge appears while at least one account
// holds a live push connection, distinguishing it from plain polling.
const { anyConnected: pushConnected } = usePushStatus();

// Work Offline state is shared (see useWorkOffline): this bar owns the toggle,
// the outbox list reads the same flag. Load the persisted value on mount.
const { offline, load, toggle } = useWorkOffline();
onMounted(() => {
  void load();
});

/** Toggles Work Offline; a backend failure is swallowed (the flag rolls back). */
async function onToggleOffline(): Promise<void> {
  try {
    await toggle();
  } catch {
    // The composable already rolled the flag back; nothing to surface here.
  }
}

/** Accessible summary of the running indicator. */
function syncLabel(): string {
  return t("layout.statusBar.syncLabel", { count: props.syncCount }, props.syncCount);
}
</script>

<template>
  <footer
    class="status"
    :aria-label="t('layout.statusBar.status')"
  >
    <span class="app">
      axomail<template v-if="version !== null"> {{ version }}</template>
    </span>
    <span
      v-if="error"
      class="error"
      role="alert"
    >
      {{ error }}
      <button
        type="button"
        class="dismiss"
        :aria-label="t('layout.statusBar.dismissError')"
        @click="emit('dismissError')"
      >
        ✕
      </button>
    </span>
    <span
      v-else-if="browserPreview"
      class="mode"
    >
      {{ t("layout.statusBar.browserPreview") }}
    </span>
    <div class="controls">
      <LayoutSwitcher />
      <span
        v-if="pushConnected && !browserPreview"
        class="push"
        :title="t('layout.statusBar.pushTitle')"
        :aria-label="t('layout.statusBar.pushConnectedAria')"
      >
        {{ t("layout.statusBar.push") }}
      </span>
      <button
        v-if="!browserPreview"
        type="button"
        class="offline-toggle"
        :class="{ active: offline }"
        :aria-pressed="offline"
        :aria-label="
          offline
            ? t('layout.statusBar.offlineToggleOffAria')
            : t('layout.statusBar.offlineToggleOnAria')
        "
        :title="offline ? t('layout.statusBar.offlineTitle') : t('layout.statusBar.workOfflineTitle')"
        @click="onToggleOffline"
      >
        {{ offline ? t("layout.statusBar.offline") : t("layout.statusBar.online") }}
      </button>
      <span
        v-if="syncing"
        class="sync"
      >
        <button
          type="button"
          class="sync-open"
          :aria-label="syncLabel()"
          @click="emit('openActivity')"
        >
          {{ t("layout.statusBar.syncing", { count: syncCount }) }}
        </button>
        <button
          type="button"
          class="cancel"
          :aria-label="t('activity.cancelAllSyncAria')"
          :title="t('activity.cancelAllSyncAria')"
          @click="emit('cancelSync')"
        >
          ✕
        </button>
      </span>
      <button
        type="button"
        class="activity"
        :aria-label="t('activity.title')"
        :title="t('activity.title')"
        @click="emit('openActivity')"
      >
        ⟳
      </button>
      <button
        type="button"
        class="settings"
        :aria-label="t('layout.statusBar.settings')"
        :title="t('layout.statusBar.settings')"
        @click="emit('openSettings')"
      >
        ⚙
      </button>
    </div>
  </footer>
</template>

<style scoped>
.status {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 0 var(--space-3);
  height: 28px;
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.mode {
  color: var(--color-text-secondary);
}

.controls {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-left: auto;
}

.offline-toggle {
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 2px var(--space-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border);
  transition: background var(--transition-fast);
}

.offline-toggle:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.offline-toggle.active {
  color: var(--color-danger);
  border-color: var(--color-danger);
  font-weight: var(--weight-medium);
}

.push {
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-accent);
  padding: 2px var(--space-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-accent);
  font-weight: var(--weight-medium);
}

.sync {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.sync-open {
  font-size: var(--text-xs);
  color: var(--color-accent);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
}

.sync-open:hover {
  background: var(--color-bg-hover);
}

.cancel {
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
}

.cancel:hover {
  background: var(--color-bg-hover);
  color: var(--color-danger);
}

.activity,
.settings {
  flex-shrink: 0;
  font-size: var(--text-sm);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
  transition: background var(--transition-fast);
}

.activity:hover,
.settings:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.error {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dismiss {
  flex-shrink: 0;
  font-size: var(--text-xs);
  line-height: 1;
  color: var(--color-danger);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
}

.dismiss:hover {
  background: var(--color-bg-hover);
}
</style>
