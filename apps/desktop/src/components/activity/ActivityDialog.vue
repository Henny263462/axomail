<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";

import { cancelAllSyncJobs, cancelSyncJob, listSyncJobs } from "../../api/jobs";
import { subscribeSyncEvents } from "../../api/sync";
import type { SyncJobDto } from "../../types/mail";
import JobRow from "./JobRow.vue";

const emit = defineEmits<{
  close: [];
}>();

type LoadState = "loading" | "ready" | "unavailable";
const state = ref<LoadState>("loading");
const jobs = ref<SyncJobDto[]>([]);
const busy = ref(false);

const root = ref<HTMLElement | null>(null);
let dispose: (() => void) | null = null;
let disposed = false;

function isActive(job: SyncJobDto): boolean {
  return job.state === "running" || job.state === "pending";
}

/** Jobs still queued or running, in the backend's order (running first). */
const activeJobs = computed(() => jobs.value.filter(isActive));

/** Jobs that reached a terminal state, newest first (backend order). */
const recentJobs = computed(() => jobs.value.filter((job) => !isActive(job)));

async function load(): Promise<void> {
  const result = await listSyncJobs();
  if (result === null) {
    state.value = "unavailable";
    return;
  }
  jobs.value = result;
  state.value = "ready";
}

async function onCancel(jobId: number): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  try {
    await cancelSyncJob(jobId);
    await load();
  } finally {
    busy.value = false;
  }
}

async function onCancelAll(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  try {
    await cancelAllSyncJobs();
    await load();
  } finally {
    busy.value = false;
  }
}

function close(): void {
  emit("close");
}

function focusables(): HTMLElement[] {
  if (root.value === null) return [];
  return Array.from(
    root.value.querySelectorAll<HTMLElement>("button:not([disabled])"),
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
  // Keep the list live while the dialog is open: every sync event reloads it.
  const off = await subscribeSyncEvents(() => {
    void load();
  });
  if (disposed) off();
  else dispose = off;
});

onUnmounted(() => {
  disposed = true;
  dispose?.();
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
      aria-labelledby="activity-title"
    >
      <header class="head">
        <h2
          id="activity-title"
          class="title"
        >
          Activity
        </h2>
        <button
          v-if="activeJobs.length > 0"
          type="button"
          class="button danger"
          :disabled="busy"
          aria-label="Cancel all sync jobs"
          @click="onCancelAll"
        >
          Cancel all
        </button>
      </header>

      <p
        v-if="state === 'loading'"
        class="note"
        role="status"
      >
        Loading activity…
      </p>
      <p
        v-else-if="state === 'unavailable'"
        class="note"
        role="status"
      >
        Background activity requires the desktop app; it is unavailable in the
        browser preview.
      </p>
      <p
        v-else-if="jobs.length === 0"
        class="note"
        role="status"
      >
        No background activity. Sync jobs appear here while they run.
      </p>

      <div
        v-else
        class="body"
      >
        <section
          v-if="activeJobs.length > 0"
          class="group"
          aria-label="Active jobs"
        >
          <h3 class="legend">
            Active
          </h3>
          <ul class="jobs">
            <JobRow
              v-for="job in activeJobs"
              :key="job.id"
              :job="job"
              :busy="busy"
              :cancellable="true"
              @cancel="onCancel"
            />
          </ul>
        </section>

        <section
          v-if="recentJobs.length > 0"
          class="group"
          aria-label="Recent jobs"
        >
          <h3 class="legend">
            Recent
          </h3>
          <ul class="jobs">
            <JobRow
              v-for="job in recentJobs"
              :key="job.id"
              :job="job"
              :busy="busy"
              :cancellable="false"
              @cancel="onCancel"
            />
          </ul>
        </section>
      </div>

      <div class="actions">
        <button
          type="button"
          class="button"
          @click="close"
        >
          Close
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
  max-width: 480px;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
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

.group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.legend {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.jobs {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  list-style: none;
}

.actions {
  display: flex;
  justify-content: flex-end;
  margin-top: var(--space-4);
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

.button.danger {
  color: var(--color-danger);
  border-color: var(--color-danger);
}
</style>
