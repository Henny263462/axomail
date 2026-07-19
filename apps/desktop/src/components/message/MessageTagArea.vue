<script setup lang="ts">
/**
 * The message detail's tag affordance: shows the applied tags as chips and a
 * "Tag" button that opens the {@link TagPicker}. Owns the per-message option
 * state (loaded from `message_tag_options`) and persists toggles and inline
 * tag creation. Resolves to an empty, inert state in the browser preview where
 * the backend is absent (ADR-0005).
 */
import { computed, ref, watch } from "vue";

import {
  createTag,
  messageTagOptions,
  setMessageTag,
} from "../../api/tags";
import type { MessageTagOption, TagChip } from "../../types/mail";
import TagChips from "./TagChips.vue";
import TagPicker from "./TagPicker.vue";

const props = defineProps<{ messageId: number }>();

const options = ref<MessageTagOption[]>([]);
const accountId = ref<number | null>(null);
const open = ref(false);
const busy = ref(false);
const error = ref("");
const container = ref<HTMLElement | null>(null);

const appliedChips = computed<TagChip[]>(() =>
  options.value
    .filter((option) => option.applied)
    .map((option) => ({
      id: option.id,
      displayName: option.displayName,
      color: option.color,
    })),
);

// Monotonic guard: every async op captures the message id it started on, so a
// result that resolves after the user switched messages is ignored rather than
// applied to (or overwriting) the newly selected message. `loadSeq` also drops
// a slow earlier load whose response arrives after a newer one for the same
// message.
let loadSeq = 0;

async function load(): Promise<void> {
  const messageId = props.messageId;
  const seq = ++loadSeq;
  const result = await messageTagOptions(messageId);
  // Ignore a stale load: the selection moved on, or a newer load superseded it.
  if (messageId !== props.messageId || seq !== loadSeq) return;
  if (result === null) {
    options.value = [];
    accountId.value = null;
    return;
  }
  options.value = result.options;
  accountId.value = result.accountId;
}

watch(
  () => props.messageId,
  () => {
    open.value = false;
    error.value = "";
    busy.value = false;
    void load();
  },
  { immediate: true },
);

async function onToggle(tagId: number, applied: boolean): Promise<void> {
  const messageId = props.messageId;
  const option = options.value.find((candidate) => candidate.id === tagId);
  if (option === undefined) return;
  busy.value = true;
  error.value = "";
  try {
    await setMessageTag(messageId, tagId, applied);
    // Do not touch another message's options if the selection changed.
    if (messageId !== props.messageId) return;
    option.applied = applied;
  } catch (thrown) {
    if (messageId === props.messageId) error.value = messageFrom(thrown);
  } finally {
    if (messageId === props.messageId) busy.value = false;
  }
}

async function onCreate(name: string): Promise<void> {
  const messageId = props.messageId;
  const account = accountId.value;
  if (account === null) return;
  busy.value = true;
  error.value = "";
  try {
    const tag = await createTag(account, name, null);
    // If the user switched messages while the tag was being created, never
    // apply it to the now-current (different) message.
    if (messageId !== props.messageId) return;
    if (tag !== null) {
      await setMessageTag(messageId, tag.id, true);
      if (messageId !== props.messageId) return;
    }
    await load();
  } catch (thrown) {
    if (messageId === props.messageId) error.value = messageFrom(thrown);
  } finally {
    if (messageId === props.messageId) busy.value = false;
  }
}

function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (next !== null && container.value?.contains(next)) return;
  open.value = false;
}

function messageFrom(thrown: unknown): string {
  if (typeof thrown === "string") return thrown;
  if (thrown instanceof Error) return thrown.message;
  return "Could not update tags.";
}
</script>

<template>
  <div
    ref="container"
    class="tag-area"
    @focusout="onFocusOut"
  >
    <div class="row">
      <TagChips
        :tags="appliedChips"
        label="Tags on this message"
      />
      <button
        type="button"
        class="tag-button"
        aria-haspopup="true"
        :aria-expanded="open"
        @click="open = !open"
      >
        Tag
      </button>
    </div>
    <div
      v-if="open"
      class="popover"
    >
      <TagPicker
        :options="options"
        :busy="busy"
        @toggle="onToggle"
        @create="onCreate"
        @close="open = false"
      />
    </div>
    <p
      v-if="error"
      class="error"
      role="alert"
    >
      {{ error }}
    </p>
  </div>
</template>

<style scoped>
.tag-area {
  position: relative;
  margin-top: var(--space-3);
}

.row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.tag-button {
  padding: 2px var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--color-accent);
  background: var(--color-bg);
}

.tag-button:hover {
  background: var(--color-bg-hover);
}

.popover {
  position: absolute;
  z-index: 20;
  margin-top: var(--space-1);
}

.error {
  margin-top: var(--space-1);
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
