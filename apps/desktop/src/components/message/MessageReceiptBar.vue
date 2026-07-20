<script setup lang="ts">
/**
 * Reading-pane bar for a pending read-receipt (MDN) request.
 *
 * For an `"ask"` request it shows Send / Ignore controls; for a `"send"`
 * request (the "always send" policy) it sends the receipt automatically on
 * mount. The backend authoritatively re-decides the policy, so triggering a
 * send here is always safe. Mount this keyed by message id so the auto-send
 * fires once per message.
 */
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { sendMdn } from "../../api/receipts";
import type { ReceiptRequestDto } from "../../types/receipts";

const props = defineProps<{
  receipt: ReceiptRequestDto;
  messageId: number;
}>();

const { t } = useI18n({ useScope: "global" });

type Status = "prompt" | "sending" | "sent" | "failed" | "dismissed";
const status = ref<Status>(props.receipt.action === "send" ? "sending" : "prompt");

onMounted(() => {
  if (props.receipt.action === "send") {
    void send();
  }
});

async function send(): Promise<void> {
  status.value = "sending";
  const outcome = await sendMdn(props.messageId);
  if (outcome === null) {
    status.value = "failed";
    return;
  }
  // "sent" and "alreadySent" both mean nothing more to do; "suppressed" means
  // the policy declined after all (dismiss quietly); "failed" surfaces an error.
  if (outcome.status === "sent" || outcome.status === "alreadySent") {
    status.value = "sent";
  } else if (outcome.status === "suppressed") {
    status.value = "dismissed";
  } else {
    status.value = "failed";
  }
}

function dismiss(): void {
  status.value = "dismissed";
}
</script>

<template>
  <div
    v-if="status !== 'dismissed'"
    class="receipt-bar"
    role="status"
  >
    <span
      v-if="status === 'sent'"
      class="text"
    >{{ t("receipts.bar.sent") }}</span>
    <span
      v-else-if="status === 'sending'"
      class="text"
    >{{ t("receipts.bar.sending") }}</span>
    <template v-else-if="status === 'failed'">
      <span class="text error">{{ t("receipts.bar.failed") }}</span>
      <button
        type="button"
        class="bar-action"
        @click="send"
      >
        {{ t("receipts.bar.send") }}
      </button>
    </template>
    <template v-else>
      <span class="text">{{ t("receipts.bar.requested") }}</span>
      <span class="actions">
        <button
          type="button"
          class="bar-action"
          @click="send"
        >
          {{ t("receipts.bar.send") }}
        </button>
        <button
          type="button"
          class="bar-action ghost"
          @click="dismiss"
        >
          {{ t("receipts.bar.ignore") }}
        </button>
      </span>
    </template>
  </div>
</template>

<style scoped>
.receipt-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-accent-subtle);
  border: 1px solid var(--color-border);
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.text.error {
  color: var(--color-danger);
}

.actions {
  display: inline-flex;
  gap: var(--space-2);
}

.bar-action {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  font-size: var(--text-xs);
  color: var(--color-accent);
  cursor: pointer;
}

.bar-action:hover {
  background: var(--color-bg-hover);
}

.bar-action.ghost {
  color: var(--color-text-secondary);
}
</style>
