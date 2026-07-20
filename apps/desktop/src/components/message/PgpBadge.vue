<script setup lang="ts">
/**
 * OpenPGP status badge for the reading pane (Track 5.1).
 *
 * On opening a message it asks the backend (`analyze_message_pgp`) whether the
 * message is signed and/or encrypted, and shows a verified/invalid/unknown-key
 * signature badge. For an encrypted message it offers a Decrypt action; if the
 * secret key is passphrase-protected the user enters the passphrase here (read
 * transiently for that one call, never stored in reactive state). A successfully
 * decrypted body is shown through the SAME renderer as normal mail
 * ({@link MessageBody}): plaintext as text and any HTML body — already sanitized
 * by the backend — inside the null-origin, script-less sandbox iframe (ADR-0007),
 * never injected as raw HTML.
 */
import { onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { analyzeMessagePgp } from "../../api/pgp";
import type { PgpMessageStatusDto } from "../../types/pgp";
import MessageBody from "./MessageBody.vue";

const props = defineProps<{ messageId: number }>();
const { t } = useI18n({ useScope: "global" });

const status = ref<PgpMessageStatusDto | null>(null);
const busy = ref(false);
// The passphrase is read straight off this (uncontrolled) input only for the
// duration of a decrypt call and then wiped — it never lives in a reactive ref,
// so it cannot be inspected in devtools or linger in component state after use.
const passphraseInput = ref<HTMLInputElement | null>(null);

// Monotonic guard: every analyze/decrypt captures a token and the message id it
// was issued for; a result is applied only if it is still the latest request
// for the still-selected message. A slow decrypt for a previously-open message
// therefore never renders its plaintext over a newer selection.
let requestToken = 0;

async function analyze(pass: string | null): Promise<void> {
  const token = ++requestToken;
  const id = props.messageId;
  busy.value = true;
  try {
    const result = await analyzeMessagePgp(id, pass);
    // Discard a stale result: a newer request superseded this one, or the
    // selection changed while it was in flight.
    if (token !== requestToken || id !== props.messageId) return;
    status.value = result;
  } finally {
    if (token === requestToken) busy.value = false;
  }
}

/** Reads the entered passphrase, decrypts, then wipes it from the field. */
async function onDecrypt(): Promise<void> {
  const input = passphraseInput.value;
  const pass = input?.value ?? "";
  try {
    await analyze(pass.length > 0 ? pass : null);
  } finally {
    // Clear the passphrase after the attempt (success or failure); it lived
    // only in the transient `pass` local, never in reactive state.
    if (passphraseInput.value) passphraseInput.value.value = "";
  }
}

/** Drops decrypted plaintext + passphrase so nothing survives the transition. */
function clearSensitive(): void {
  status.value = null;
  if (passphraseInput.value) passphraseInput.value.value = "";
}

watch(
  () => props.messageId,
  (id) => {
    clearSensitive();
    if (Number.isFinite(id)) void analyze(null);
  },
  { immediate: true },
);

onUnmounted(clearSensitive);

/** The signature chip's label + tone, or null when unsigned. Only a signature
 * that is cryptographically valid AND matched to the sender's key is shown
 * green ("ok"); every other outcome — including an unknown or non-sender
 * signer — is a non-green warning, never a plain "valid" badge. */
function signatureChip(
  s: PgpMessageStatusDto,
): { label: string; tone: string } | null {
  if (!s.signed) return null;
  switch (s.signatureStatus) {
    case "valid":
      return {
        label: s.signer
          ? t("pgp.badge.signedValidBy", { signer: s.signer })
          : t("pgp.badge.signedValid"),
        tone: "ok",
      };
    case "invalid":
      return { label: t("pgp.badge.signedInvalid"), tone: "bad" };
    case "signerMismatch":
      return { label: t("pgp.badge.signedMismatch"), tone: "warn" };
    default:
      return { label: t("pgp.badge.signedUnknown"), tone: "warn" };
  }
}
</script>

<template>
  <section
    v-if="status && status.isPgp"
    class="pgp-badge"
    role="status"
  >
    <div class="chips">
      <span
        v-if="status.encrypted"
        class="chip ok"
      >{{
        status.decrypted ? t("pgp.badge.decrypted") : t("pgp.badge.encrypted")
      }}</span>
      <span
        v-if="signatureChip(status)"
        class="chip"
        :class="signatureChip(status)?.tone"
      >{{ signatureChip(status)?.label }}</span>
    </div>

    <div
      v-if="status.encrypted && !status.decrypted && status.needsPassphrase"
      class="unlock"
    >
      <label class="unlock-label">{{ t("pgp.badge.needsPassphrase") }}</label>
      <div class="unlock-row">
        <input
          ref="passphraseInput"
          type="password"
          :aria-label="t('pgp.badge.needsPassphrase')"
          @keydown.enter.prevent="onDecrypt"
        >
        <button
          type="button"
          :disabled="busy"
          @click="onDecrypt"
        >
          {{ t("pgp.badge.decrypt") }}
        </button>
      </div>
    </div>

    <p
      v-else-if="status.encrypted && !status.decrypted && status.error"
      class="error"
    >
      {{ t("pgp.badge.failed") }}
    </p>

    <!--
      Decrypted content is rendered through the SAME text/sandboxed-iframe path
      as normal mail (ADR-0007): the plaintext shows as text and any HTML body
      (already sanitized by the backend) is shown in a null-origin script-less
      iframe — never injected as raw HTML — so an HTML-only message is no longer
      blank.
    -->
    <MessageBody
      v-if="status.decrypted && (status.bodyText || status.bodyHtml)"
      :text-body="status.bodyText"
      :sanitized-html="status.bodyHtml"
    />
  </section>
</template>

<style scoped>
.pgp-badge {
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.chip {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text-secondary);
}

.chip.ok {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

.chip.bad {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.chip.warn {
  color: var(--color-text-secondary);
}

.unlock {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.unlock-label {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.unlock-row {
  display: flex;
  gap: var(--space-2);
}

.unlock-row input {
  flex: 1 1 auto;
  font-size: var(--text-sm);
}

.error {
  font-size: var(--text-xs);
  color: var(--color-danger);
}
</style>
