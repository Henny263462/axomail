<script setup lang="ts">
import { onMounted } from "vue";
import { useI18n } from "vue-i18n";

import { useAppVersion } from "../../composables/useAppVersion";
import { usePushStatus } from "../../composables/usePushStatus";
import { useWorkOffline } from "../../composables/useWorkOffline";
import { isTauri } from "../../lib/tauri";
import AppIcon from "../common/AppIcon.vue";

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
      <span
        class="dot"
        :class="{ live: pushConnected && !browserPreview, busy: syncing }"
        aria-hidden="true"
      />
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
        <AppIcon
          name="close"
          :size="12"
        />
      </button>
    </span>
    <span
      v-else-if="browserPreview"
      class="mode"
    >
      {{ t("layout.statusBar.browserPreview") }}
    </span>
    <div class="controls">
      <span
        v-if="pushConnected && !browserPreview"
        class="push"
        :title="t('layout.statusBar.pushTitle')"
        :aria-label="t('layout.statusBar.pushConnectedAria')"
      >
        <AppIcon
          name="zap"
          :size="12"
        />
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
        <AppIcon
          :name="offline ? 'wifiOff' : 'wifi'"
          :size="13"
        />
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
          <AppIcon
            name="refresh"
            class="ax-spin"
            :size="12"
          />
          {{ t("layout.statusBar.syncing", { count: syncCount }) }}
        </button>
        <button
          type="button"
          class="cancel"
          :aria-label="t('activity.cancelAllSyncAria')"
          :title="t('activity.cancelAllSyncAria')"
          @click="emit('cancelSync')"
        >
          <AppIcon
            name="close"
            :size="12"
          />
        </button>
      </span>
      <button
        type="button"
        class="activity"
        :aria-label="t('activity.title')"
        :title="t('activity.title')"
        @click="emit('openActivity')"
      >
        <AppIcon
          name="activity"
          :size="15"
        />
      </button>
      <button
        type="button"
        class="settings"
        :aria-label="t('layout.statusBar.settings')"
        :title="t('layout.statusBar.settings')"
        @click="emit('openSettings')"
      >
        <AppIcon
          name="settings"
          :size="15"
        />
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
  height: 32px;
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-subtle);
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.app {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-weight: var(--weight-medium);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--color-text-tertiary);
  transition: background var(--duration-base) ease;
}

.dot.live {
  background: var(--color-success);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-success) 20%, transparent);
}

.dot.busy {
  background: var(--color-accent);
  animation: ax-pulse 1.4s ease-in-out infinite;
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--color-accent);
  padding: 2px var(--space-1);
  border-radius: var(--radius-sm);
}

.sync-open:hover {
  background: var(--color-bg-hover);
}

.cancel {
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  font-size: var(--text-sm);
  line-height: 1;
  color: var(--color-text-secondary);
  padding: 3px;
  border-radius: var(--radius-sm);
}

.activity:hover,
.settings:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.settings:hover :deep(.icon) {
  rotate: 90deg;
}

.settings :deep(.icon) {
  transition: rotate var(--duration-slow) var(--ease-out);
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
