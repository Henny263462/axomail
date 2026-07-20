<script setup lang="ts">
/**
 * Folder picker for Move to… / Copy to…. Lists every account's folders; picking
 * one resolves the pending move or copy of `ids`. Focus is trapped and Escape
 * cancels, via the shared {@link useFocusTrap}.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useFocusTrap } from "../../composables/useFocusTrap";
import type { FolderPicked } from "../../composables/useMailboxActions";
import type { AccountSummary } from "../../types/mail";

const { t } = useI18n({ useScope: "global" });

const props = defineProps<{
  accounts: AccountSummary[];
  ids: number[];
  mode: "move" | "copy";
}>();

const emit = defineEmits<{
  close: [];
  picked: [payload: FolderPicked];
}>();

const root = ref<HTMLElement | null>(null);
let invoker: HTMLElement | null = null;

const { onKeydown } = useFocusTrap(root, () => emit("close"));

onMounted(() => {
  invoker = document.activeElement as HTMLElement | null;
  // Focus the first folder, or the always-present Cancel button when an account
  // has no folders, so the trap has an anchor and Escape reaches the overlay.
  const target =
    root.value?.querySelector<HTMLElement>(".folder") ??
    root.value?.querySelector<HTMLElement>(".button");
  target?.focus();
});

onBeforeUnmount(() => invoker?.focus());

function pick(mailboxId: number, mailboxName: string): void {
  emit("picked", { ids: props.ids, mode: props.mode, mailboxId, mailboxName });
}
</script>

<template>
  <div
    class="overlay"
    @keydown="onKeydown"
  >
    <div
      ref="root"
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="move-dialog-title"
    >
      <h3
        id="move-dialog-title"
        class="title"
      >
        {{ mode === "move" ? t("message.move.moveTitle") : t("message.move.copyTitle") }}
      </h3>

      <div
        v-if="accounts.length === 0"
        class="empty"
      >
        {{ t("message.move.noFolders") }}
      </div>

      <div
        v-for="account in accounts"
        :key="account.id"
        class="group"
      >
        <p class="account">
          {{ account.displayName }}
        </p>
        <ul class="folders">
          <li
            v-for="mailbox in account.mailboxes"
            :key="mailbox.id"
          >
            <button
              type="button"
              class="folder"
              @click="pick(mailbox.id, mailbox.name)"
            >
              {{ mailbox.name }}
            </button>
          </li>
        </ul>
      </div>

      <div class="actions">
        <button
          type="button"
          class="button"
          @click="emit('close')"
        >
          {{ t("common.cancel") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: var(--space-6) var(--space-4);
  overflow-y: auto;
  background: rgb(0 0 0 / 40%);
}

.dialog {
  width: 100%;
  max-width: 380px;
  max-height: 80vh;
  overflow-y: auto;
  background: var(--color-bg);
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-lg);
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
}

.empty {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.account {
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-text-tertiary);
  margin-bottom: var(--space-1);
}

.folders {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.folder {
  display: block;
  width: 100%;
  text-align: left;
  font-size: var(--text-sm);
  color: var(--color-text);
  background: var(--color-bg);
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  padding: var(--space-1) var(--space-2);
  cursor: pointer;
}

.folder:hover,
.folder:focus-visible {
  background: var(--color-bg-hover);
  border-color: var(--color-accent);
}

.actions {
  display: flex;
  justify-content: flex-end;
}

.button {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text);
  background: var(--color-bg);
}

.button:hover {
  background: var(--color-bg-hover);
}
</style>
