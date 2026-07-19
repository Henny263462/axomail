<script setup lang="ts">
/**
 * The composer's attachment strip: an "Attach files" button, a chip per chosen
 * file (name + size, with remove), and best-effort drag-and-drop.
 *
 * Files are identified by their absolute path (the model value); the bytes are
 * read in Rust at send time, never here (ADR-0010). The native dialog is the
 * reliable path; OS file drops are a convenience. A running total is capped so
 * an oversized selection is rejected before it ever reaches the send command.
 */
import { onBeforeUnmount, onMounted, computed } from "vue";

import { onFilesDropped, pickAttachments, statAttachments } from "../../api/attachments";
import type { AttachmentCandidateDto } from "../../types/mail";

/** Combined attachment cap, mirroring the backend's 25 MiB limit. */
const MAX_TOTAL_BYTES = 25 * 1024 * 1024;

const attachments = defineModel<AttachmentCandidateDto[]>({ default: () => [] });

const emit = defineEmits<{
  error: [message: string];
}>();

const totalBytes = computed(() =>
  attachments.value.reduce((sum, item) => sum + item.sizeBytes, 0),
);

let stopDrop: (() => void) | null = null;

onMounted(() => {
  stopDrop = onFilesDropped((paths) => void addPaths(paths));
});

onBeforeUnmount(() => {
  stopDrop?.();
});

/** Opens the native picker and adds whatever the user selects. */
async function pick(): Promise<void> {
  const paths = await pickAttachments();
  if (paths !== null) await addPaths(paths);
}

/** Stats new paths, enforces the cap, and appends the survivors as chips. */
async function addPaths(paths: string[]): Promise<void> {
  const known = new Set(attachments.value.map((item) => item.path));
  const fresh = paths.filter((path) => !known.has(path));
  if (fresh.length === 0) return;

  const metas = await statAttachments(fresh);
  if (metas === null) {
    emit("error", "Attachments need the desktop app; they are unavailable in the browser preview.");
    return;
  }
  const added = metas.reduce((sum, item) => sum + item.sizeBytes, 0);
  if (totalBytes.value + added > MAX_TOTAL_BYTES) {
    emit(
      "error",
      `Attachments would exceed the ${formatSize(MAX_TOTAL_BYTES)} limit; some files were not added.`,
    );
    return;
  }
  attachments.value = [...attachments.value, ...metas];
}

/** Removes one attachment by path. */
function remove(path: string): void {
  attachments.value = attachments.value.filter((item) => item.path !== path);
}

/** Formats a byte count as a compact, human-readable size. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(kib < 10 ? 1 : 0)} KB`;
  const mib = kib / 1024;
  return `${mib.toFixed(mib < 10 ? 1 : 0)} MB`;
}
</script>

<template>
  <div class="attachments">
    <div class="head">
      <button
        type="button"
        class="attach-button"
        @click="pick"
      >
        📎 Attach files
      </button>
      <span
        v-if="attachments.length > 0"
        class="total"
      >
        {{ attachments.length }} file{{ attachments.length === 1 ? "" : "s" }} ·
        {{ formatSize(totalBytes) }}
      </span>
    </div>

    <ul
      v-if="attachments.length > 0"
      class="chips"
      aria-label="Attachments"
    >
      <li
        v-for="item in attachments"
        :key="item.path"
        class="chip"
      >
        <span class="chip-name">{{ item.filename }}</span>
        <span class="chip-size">{{ formatSize(item.sizeBytes) }}</span>
        <button
          type="button"
          class="chip-remove"
          :aria-label="`Remove ${item.filename}`"
          @click="remove(item.path)"
        >
          ×
        </button>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.attachments {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.attach-button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  padding: var(--space-1) var(--space-3);
  cursor: pointer;
}

.attach-button:hover {
  background: var(--color-bg-hover);
}

.total {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  list-style: none;
  padding: 0;
  margin: 0;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  font-size: var(--text-xs);
  background: var(--color-bg-subtle);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
}

.chip-name {
  font-weight: var(--weight-medium);
  color: var(--color-text);
  max-width: 16rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chip-size {
  color: var(--color-text-secondary);
}

.chip-remove {
  font-size: var(--text-sm);
  line-height: 1;
  color: var(--color-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  padding: 0 var(--space-1);
}

.chip-remove:hover {
  color: var(--color-danger);
}
</style>
