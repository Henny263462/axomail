<script setup lang="ts">
/**
 * The reading-pane action toolbar: Reply / Reply all / Forward, then the
 * relocation + Junk + Delete actions. Split out of {@link MessageDetail} to keep
 * that file under the size gate. Pure presentation — each control emits and the
 * host (MessageDetail → MessageViewPane) performs the action.
 */
const emit = defineEmits<{
  (event: "reply"): void;
  (event: "reply-all"): void;
  (event: "forward"): void;
  (event: "archive"): void;
  (event: "move"): void;
  (event: "copy"): void;
  (event: "junk"): void;
  (event: "delete"): void;
}>();
</script>

<template>
  <div
    class="action-bar"
    role="toolbar"
    aria-label="Message actions"
  >
    <button
      type="button"
      class="action"
      @click="emit('reply')"
    >
      Reply
    </button>
    <button
      type="button"
      class="action"
      @click="emit('reply-all')"
    >
      Reply all
    </button>
    <button
      type="button"
      class="action"
      @click="emit('forward')"
    >
      Forward
    </button>
    <span
      class="action-sep"
      aria-hidden="true"
    />
    <button
      type="button"
      class="action"
      @click="emit('archive')"
    >
      Archive
    </button>
    <button
      type="button"
      class="action"
      @click="emit('move')"
    >
      Move to…
    </button>
    <button
      type="button"
      class="action"
      @click="emit('copy')"
    >
      Copy to…
    </button>
    <button
      type="button"
      class="action"
      @click="emit('junk')"
    >
      Junk
    </button>
    <button
      type="button"
      class="action danger"
      @click="emit('delete')"
    >
      Delete
    </button>
  </div>
</template>

<style scoped>
.action-bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-bottom: var(--space-4);
}

.action {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-sm);
  color: var(--color-text);
  cursor: pointer;
}

.action:hover {
  background: var(--color-bg-hover);
}

.action:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

.action.danger {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.action.danger:hover {
  background: var(--color-danger);
  color: var(--color-text-inverse);
}

.action-sep {
  width: 1px;
  align-self: stretch;
  margin: 0 var(--space-1);
  background: var(--color-border);
}
</style>
