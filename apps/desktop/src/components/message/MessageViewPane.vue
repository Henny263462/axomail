<script setup lang="ts">
import type { MessageListItem } from "../../types/mail";
import EmptyState from "../common/EmptyState.vue";

defineProps<{
  message: MessageListItem | null;
}>();
</script>

<template>
  <section
    class="view"
    aria-label="Message"
  >
    <EmptyState
      v-if="message === null"
      title="No message selected"
      detail="Select a message from the list to read it here."
    />
    <article
      v-else
      class="content"
    >
      <header class="header">
        <h1 class="subject">
          {{ message.subject ?? "(no subject)" }}
        </h1>
        <p class="meta">
          <span class="from">{{ message.fromName }}</span>
          <span class="address">&lt;{{ message.fromAddress }}&gt;</span>
        </p>
      </header>
      <!--
        Body rendering is deliberately absent: it requires the sanitizing
        pipeline decided in a dedicated ADR before phase 2. Until then
        only envelope data is shown.
      -->
      <p class="body-note">
        Message bodies are rendered once the sanitized viewing pipeline
        lands (phase 2).
      </p>
    </article>
  </section>
</template>

<style scoped>
.view {
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg);
}

.content {
  padding: var(--space-5);
}

.header {
  border-bottom: 1px solid var(--color-border);
  padding-bottom: var(--space-3);
  margin-bottom: var(--space-4);
}

.subject {
  font-size: var(--text-lg);
  font-weight: var(--weight-semibold);
  letter-spacing: -0.01em;
}

.meta {
  margin-top: var(--space-1);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.address {
  color: var(--color-text-tertiary);
  margin-left: var(--space-1);
}

.body-note {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}
</style>
