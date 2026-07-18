<script setup lang="ts">
import { ref, watch } from "vue";

import { getMessageDetail } from "../../api/messages";
import type { MessageDetailDto } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";
import MessageDetail from "./MessageDetail.vue";

const props = defineProps<{
  messageId: number | null;
}>();

type LoadState = "idle" | "loading" | "loaded" | "error";

const state = ref<LoadState>("idle");
const detail = ref<MessageDetailDto | null>(null);

/**
 * Loads the detail for `id`, ignoring the result if the selection changed
 * while the request was in flight (last-write-wins by comparing ids).
 */
async function load(id: number): Promise<void> {
  state.value = "loading";
  detail.value = null;
  const result = await getMessageDetail(id);
  if (props.messageId !== id) return;
  if (result === null) {
    state.value = "error";
    return;
  }
  detail.value = result;
  state.value = "loaded";
}

watch(
  () => props.messageId,
  (id) => {
    if (id === null) {
      state.value = "idle";
      detail.value = null;
      return;
    }
    void load(id);
  },
  { immediate: true },
);
</script>

<template>
  <section
    class="view"
    aria-label="Message"
  >
    <EmptyState
      v-if="state === 'idle'"
      title="No message selected"
      detail="Select a message from the list to read it here."
    />
    <p
      v-else-if="state === 'loading'"
      class="status"
      role="status"
    >
      Loading message…
    </p>
    <EmptyState
      v-else-if="state === 'error' || detail === null"
      title="Message unavailable"
      detail="This message could not be loaded. It may require the desktop app."
    />
    <MessageDetail
      v-else
      :detail="detail"
    />
  </section>
</template>

<style scoped>
.view {
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg);
}

.status {
  padding: var(--space-5);
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}
</style>
