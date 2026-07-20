<script setup lang="ts">
import { onMounted } from "vue";

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
  return props.syncCount === 1
    ? "1 sync job running — open Activity"
    : `${props.syncCount} sync jobs running — open Activity`;
}
</script>

<template>
  <footer
    class="status"
    aria-label="Status"
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
        aria-label="Dismiss error"
        @click="emit('dismissError')"
      >
        ✕
      </button>
    </span>
    <span
      v-else-if="browserPreview"
      class="mode"
    >
      Browser preview — mail features require the desktop app
    </span>
    <div class="controls">
      <LayoutSwitcher />
      <span
        v-if="pushConnected && !browserPreview"
        class="push"
        title="Connected — receiving new mail by push (IMAP IDLE)"
        aria-label="Push connected"
      >
        Push
      </span>
      <button
        v-if="!browserPreview"
        type="button"
        class="offline-toggle"
        :class="{ active: offline }"
        :aria-pressed="offline"
        :aria-label="
          offline
            ? 'Working offline — click to go back online'
            : 'Working online — click to work offline'
        "
        :title="offline ? 'Working offline' : 'Work offline'"
        @click="onToggleOffline"
      >
        {{ offline ? "Offline" : "Online" }}
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
          Syncing {{ syncCount }}…
        </button>
        <button
          type="button"
          class="cancel"
          aria-label="Cancel all sync jobs"
          title="Cancel all sync jobs"
          @click="emit('cancelSync')"
        >
          ✕
        </button>
      </span>
      <button
        type="button"
        class="activity"
        aria-label="Activity"
        title="Activity"
        @click="emit('openActivity')"
      >
        ⟳
      </button>
      <button
        type="button"
        class="settings"
        aria-label="Settings"
        title="Settings"
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
