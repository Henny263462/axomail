<script setup lang="ts">
/**
 * The composer's message-body editor: a dependency-free rich-text surface with
 * a plain-text fallback.
 *
 * In HTML mode a `contenteditable` div is edited through {@link ComposeToolbar}'s
 * `document.execCommand` commands, but nothing the browser produces is trusted:
 * every read of the surface is reduced by {@link sanitizeEditorHtml} to the same
 * allowlisted subset the reader enforces (ADR-0007), and the Rust send path
 * re-sanitizes authoritatively. The component emits BOTH a sanitized HTML body
 * (`html`, `null` in plain mode) and a degraded plain-text alternative (`text`,
 * always present) — the body *without* the signature, which the shell appends
 * per identity so it is never duplicated across a draft round-trip.
 *
 * The plain-text toggle degrades HTML → text; toggling back rebuilds a best-
 * effort HTML view from the text.
 */
import { computed, nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { SIGNATURE_DELIMITER } from "../../lib/signature";
import {
  htmlToText,
  sanitizeEditorHtml,
  textToHtml,
  urlScheme,
} from "../../lib/htmlBody";
import ComposeToolbar from "./ComposeToolbar.vue";

const props = defineProps<{
  /** The selected identity's plain signature, shown as an appended-on-send
   * preview. The shell owns the actual merge so drafts never double it. */
  signatureText?: string | null;
  /** True when editing a stored (text-only) draft: opens in plain-text mode. */
  isDraft?: boolean;
}>();

/** Plain-text alternative body (always current), without the signature. */
const text = defineModel<string>("text", { default: "" });
/** Sanitized HTML body (without the signature); `null` in plain-text mode. */
const html = defineModel<string | null>("html", { default: null });

const { t } = useI18n({ useScope: "global" });

const mode = ref<"html" | "plain">(props.isDraft === true ? "plain" : "html");
const editor = ref<HTMLElement | null>(null);

/** The signature shown beneath the body (delimiter line, then the text). */
const signaturePreview = computed(
  () => `${SIGNATURE_DELIMITER}\n${props.signatureText ?? ""}`,
);
/** Whether a non-blank signature applies to this message. */
const hasSignature = computed(
  () => (props.signatureText ?? "").trim().length > 0,
);

onMounted(() => {
  if (mode.value === "html") {
    void nextTick(seedEditor);
  }
});

/** Loads the current model value into the contenteditable surface. */
function seedEditor(): void {
  if (editor.value === null) return;
  editor.value.innerHTML = html.value ?? textToHtml(text.value);
  syncFromEditor();
}

/** Re-reads and sanitizes the surface, updating both emitted bodies. */
function syncFromEditor(): void {
  if (editor.value === null) return;
  const cleaned = sanitizeEditorHtml(editor.value.innerHTML);
  html.value = cleaned;
  text.value = htmlToText(cleaned);
}

/** Runs a formatting command against the surface, then re-syncs. */
function runCommand(name: string): void {
  editor.value?.focus();
  const parts = name.split(":");
  exec(parts[0] ?? name, parts[1]);
  syncFromEditor();
}

/** Wraps the current selection in a sanitized-scheme hyperlink. */
function insertLink(url: string): void {
  const safe = normalizeUrl(url);
  if (safe === null) return;
  editor.value?.focus();
  exec("createLink", safe);
  syncFromEditor();
}

/** Invokes `document.execCommand` defensively (a no-op where unsupported). */
function exec(command: string, value?: string): void {
  if (typeof document.execCommand !== "function") return;
  try {
    document.execCommand(command, false, value);
  } catch {
    // Some environments (and the test DOM) reject execCommand; ignore.
  }
}

/** Coerces a user-typed URL into an allowed scheme, or `null` if unusable. */
function normalizeUrl(url: string): string | null {
  const trimmed = url.trim();
  if (trimmed.length === 0) return null;
  const scheme = urlScheme(trimmed);
  if (scheme === "http" || scheme === "https" || scheme === "mailto") {
    return trimmed;
  }
  if (scheme !== null) return null; // A disallowed scheme (javascript:, etc.).
  return trimmed.includes("@") && !trimmed.includes("/")
    ? `mailto:${trimmed}`
    : `https://${trimmed}`;
}

/** Switches between the rich and plain-text editing surfaces. */
function toggleMode(): void {
  if (mode.value === "html") {
    // `text` already mirrors the surface; drop the HTML alternative.
    html.value = null;
    mode.value = "plain";
    return;
  }
  mode.value = "html";
  html.value = textToHtml(text.value);
  void nextTick(seedEditor);
}
</script>

<template>
  <div class="field">
    <div class="body-head">
      <label :for="mode === 'html' ? 'compose-editor' : 'compose-body'">
        {{ t("compose.body.label") }}
      </label>
      <button
        type="button"
        class="mode-toggle"
        :aria-pressed="mode === 'plain'"
        @click="toggleMode"
      >
        {{ mode === "html" ? t("compose.body.plainText") : t("compose.body.richText") }}
      </button>
    </div>

    <template v-if="mode === 'html'">
      <ComposeToolbar
        @command="runCommand"
        @link="insertLink"
      />
      <div
        id="compose-editor"
        ref="editor"
        class="editor"
        contenteditable="true"
        role="textbox"
        aria-multiline="true"
        :aria-label="t('compose.body.bodyAria')"
        :data-placeholder="t('compose.body.placeholder')"
        @input="syncFromEditor"
      />
    </template>
    <textarea
      v-else
      id="compose-body"
      v-model="text"
      rows="10"
      :aria-label="t('compose.body.bodyAria')"
    />

    <p
      v-if="hasSignature"
      class="signature-preview"
    >
      <span class="signature-label">{{ t("compose.body.signatureLabel") }}</span>
      <span class="signature-body">{{ signaturePreview }}</span>
    </p>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.body-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.field label {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-secondary);
}

.mode-toggle {
  font-size: var(--text-xs);
  color: var(--color-accent);
  background: none;
  border: none;
  cursor: pointer;
  padding: 0;
}

.editor {
  font: inherit;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: 0 0 var(--radius-sm) var(--radius-sm);
  padding: var(--space-2);
  min-height: 10rem;
  overflow-y: auto;
  outline: none;
}

.editor:focus-visible {
  border-color: var(--color-accent);
}

.editor:empty::before {
  content: attr(data-placeholder);
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

.signature-preview {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  border-left: 2px solid var(--color-border);
  padding-left: var(--space-2);
}

.signature-label {
  font-weight: var(--weight-medium);
}

.signature-body {
  white-space: pre-wrap;
  font-family: var(--font-mono, monospace);
}
</style>
