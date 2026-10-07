<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import AppIcon from "../common/AppIcon.vue";
import { framedHtml, readZoom, writeZoom, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from "./htmlFrame";

const { t } = useI18n({ useScope: "global" });

const props = defineProps<{
  textBody: string | null;
  sanitizedHtml: string | null;
}>();

type BodyTab = "text" | "html";

const hasHtml = computed(() => props.sanitizedHtml !== null);

const activeTab = ref<BodyTab>(hasHtml.value ? "html" : "text");

watch(hasHtml, (present) => {
  activeTab.value = present ? "html" : "text";
});

const hasLinks = computed(() => props.sanitizedHtml?.includes("<a ") ?? false);

const zoom = ref(readZoom());
const zoomLabel = computed(() => `${Math.round(zoom.value * 100)}%`);
const srcdoc = computed(() => framedHtml(props.sanitizedHtml ?? "", zoom.value));

function setZoom(next: number): void {
  zoom.value = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(next * 100) / 100));
  writeZoom(zoom.value);
}
</script>

<template>
  <section
    class="body"
    :aria-label="t('message.body.ariaLabel')"
  >
    <div class="viewer-bar">
      <div
        class="tabs"
        role="tablist"
        :aria-label="t('message.body.displayMode')"
      >
        <button
          v-if="hasHtml"
          type="button"
          role="tab"
          class="tab"
          :class="{ active: activeTab === 'html' }"
          :aria-selected="activeTab === 'html'"
          @click="activeTab = 'html'"
        >
          {{ t("message.body.sanitizedHtml") }}
        </button>
        <button
          type="button"
          role="tab"
          class="tab"
          :class="{ active: activeTab === 'text' }"
          :aria-selected="activeTab === 'text'"
          @click="activeTab = 'text'"
        >
          {{ t("message.body.plainText") }}
        </button>
      </div>
      <div
        v-if="activeTab === 'html'"
        class="zoom"
        role="group"
        :aria-label="t('message.body.zoom')"
      >
        <button
          type="button"
          class="zoom-button"
          :disabled="zoom <= ZOOM_MIN"
          :aria-label="t('message.body.zoomOut')"
          :title="t('message.body.zoomOut')"
          @click="setZoom(zoom - ZOOM_STEP)"
        >
          <AppIcon
            name="zoomOut"
            :size="15"
          />
        </button>
        <button
          type="button"
          class="zoom-value"
          :title="t('message.body.zoomReset')"
          @click="setZoom(1)"
        >
          {{ zoomLabel }}
        </button>
        <button
          type="button"
          class="zoom-button"
          :disabled="zoom >= ZOOM_MAX"
          :aria-label="t('message.body.zoomIn')"
          :title="t('message.body.zoomIn')"
          @click="setZoom(zoom + ZOOM_STEP)"
        >
          <AppIcon
            name="zoomIn"
            :size="15"
          />
        </button>
      </div>
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
        {{ t("message.body.noPlainText") }}
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
        {{ t("message.body.linksDisabled") }}
      </p>
      <!-- Sanitized in Rust, rendered in a null-origin, script-less sandbox (ADR-0007). -->
      <iframe
        class="html-frame"
        :title="t('message.body.htmlPreviewTitle')"
        sandbox=""
        :srcdoc="srcdoc"
      />
    </div>
  </section>
</template>

<style scoped>
.body {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 440px;
  margin-top: var(--space-4);
}

.viewer-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
}

.tabs {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-md);
  background: var(--color-bg-sunken);
}

.tab {
  padding: 3px var(--space-3);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.tab:hover {
  color: var(--color-text);
}

.tab.active {
  background: var(--color-bg-elevated);
  color: var(--color-text);
  font-weight: var(--weight-medium);
  box-shadow: var(--shadow-sm);
}

.zoom {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.zoom-button,
.zoom-value {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 26px;
  min-width: 26px;
  padding: 0 4px;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-variant-numeric: tabular-nums;
  color: var(--color-text-secondary);
}

.zoom-value {
  min-width: 46px;
}

.zoom-button:hover:not(:disabled),
.zoom-value:hover {
  background: var(--color-bg-hover);
  color: var(--color-text);
}

.zoom-button:disabled {
  color: var(--color-text-tertiary);
  cursor: default;
}

.text-view,
.html-view {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.text {
  flex: 1;
  margin: 0;
  padding: var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  line-height: 1.6;
  color: var(--color-text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  overflow: auto;
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
  flex: 1;
  display: block;
  width: 100%;
  min-height: 400px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: #ffffff;
}
</style>
