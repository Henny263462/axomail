<script setup lang="ts">
/**
 * The composer's message-body editor.
 *
 * Today this is a plain-text `<textarea>` bound two-way to the shell. It is a
 * standalone component so Wave 3 track 3.5 (rich compose) can replace or wrap
 * the editor — swapping in a dependency-free contenteditable surface and an
 * attachments strip — without touching the composer shell or its send/draft
 * logic. The model value stays the plain-text body the send path serializes.
 */
const body = defineModel<string>({ default: "" });
</script>

<template>
  <div class="field">
    <label for="compose-body">Message</label>
    <textarea
      id="compose-body"
      v-model="body"
      rows="10"
    />
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.field textarea {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  resize: vertical;
  min-height: 8rem;
}

.field textarea:focus-visible {
  border-color: var(--color-accent);
}
</style>
