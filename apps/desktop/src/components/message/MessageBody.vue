<script setup lang="ts">
import { computed, ref, watch } from "vue";

const props = defineProps<{
  textBody: string | null;
  sanitizedHtml: string | null;
}>();

type BodyTab = "text" | "html";

const hasHtml = computed(() => props.sanitizedHtml !== null);

// Default to HTML when present, otherwise the always-available text view.
const activeTab = ref<BodyTab>(hasHtml.value ? "html" : "text");

// Re-pick the default whenever the message changes (html present/absent).
watch(hasHtml, (present) => {
  activeTab.value = present ? "html" : "text";
});

// Cheap substring check (ADR-0007): the sandbox already makes links inert;
// this only decides whether to show the explanatory note.
const hasLinks = computed(
  () => props.sanitizedHtml?.includes("<a ") ?? false,
);
</script>

<template>
  <section
    class="body"
    aria-label="Message body"
  >
    <div
      class="tabs"
      role="tablist"
      aria-label="Body format"
    >
      <button
        type="button"
        role="tab"
        class="tab"
        :class="{ active: activeTab === 'text' }"
        :aria-selected="activeTab === 'text'"
        @click="activeTab = 'text'"
      >
        Text
      </button>
      <button
        v-if="hasHtml"
        type="button"
        role="tab"
        class="tab"
        :class="{ active: activeTab === 'html' }"
        :aria-selected="activeTab === 'html'"
        @click="activeTab = 'html'"
      >
        HTML
      </button>
    </div>

    <div
      v-if="activeTab === 'text'"
      class="text-view"
      role="tabpanel"
    >
      <pre
        v-if="textBody"
        class="text"
      >{{ textBody }}</pre>
      <p
        v-else
        class="empty-note"
      >
        This message has no plain-text part.
      </p>
    </div>

    <div
      v-else
      class="html-view"
      role="tabpanel"
    >
      <p
        v-if="hasLinks"
        class="link-note"
      >
        Links are disabled in this preview.
      </p>
      <!--
        Sanitized in Rust (axomail-security) then rendered in a null-origin,
        script-less sandbox: no allow-scripts, no allow-same-origin. Height
        is a fixed conservative minimum — no scripts are injected to measure
        (ADR-0007).
      -->
      <iframe
        class="html-frame"
        title="Message HTML preview"
        sandbox=""
        :srcdoc="sanitizedHtml ?? ''"
      />
    </div>
  </section>
</template>

<style scoped>
.body {
  margin-top: var(--space-4);
}

.tabs {
  display: flex;
  gap: var(--space-1);
  border-bottom: 1px solid var(--color-border);
  margin-bottom: var(--space-3);
}

.tab {
  padding: var(--space-1) var(--space-3);
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
  border-bottom: 2px solid transparent;
  transition: color var(--transition-fast);
}

.tab:hover {
  color: var(--color-text);
}

.tab.active {
  color: var(--color-text);
  font-weight: var(--weight-medium);
  border-bottom-color: var(--color-accent);
}

.text {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--color-text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  margin: 0;
}

.empty-note {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.link-note {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  margin-bottom: var(--space-2);
}

.html-frame {
  display: block;
  width: 100%;
  min-height: 480px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  /* Message HTML assumes a light canvas; pin one regardless of app theme. */
  background: #ffffff;
}
</style>
