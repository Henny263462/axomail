<script setup lang="ts">
import { useI18n } from "vue-i18n";

import MessageViewPane from "./components/message/MessageViewPane.vue";
import { provideCommandRegistry } from "./composables/useCommandRegistry";
import {
  closeCurrentWindow,
  sendMessageAction,
  type MessageWindowAction,
} from "./lib/messageWindow";
import type { ForwardStyle } from "./types/compose";

const props = defineProps<{ messageId: number }>();

const { t } = useI18n({ useScope: "global" });

provideCommandRegistry();

const CLOSING: ReadonlySet<MessageWindowAction> = new Set(["archive", "move", "junk", "delete"]);

function forwardAction(style?: ForwardStyle): MessageWindowAction {
  if (style === "inline") return "forwardInline";
  if (style === "attachment") return "forwardAttachment";
  return "forward";
}

async function run(action: MessageWindowAction, messageId: number): Promise<void> {
  await sendMessageAction({ action, messageId });
  if (CLOSING.has(action)) await closeCurrentWindow();
}
</script>

<template>
  <main
    class="message-window"
    :aria-label="t('messageWindow.title')"
  >
    <MessageViewPane
      :message-id="props.messageId"
      standalone
      @reply="run('reply', $event)"
      @reply-all="run('replyAll', $event)"
      @forward="(id, style) => run(forwardAction(style), id)"
      @archive="run('archive', $event)"
      @move="run('move', $event)"
      @copy="run('copy', $event)"
      @junk="run('junk', $event)"
      @delete-message="run('delete', $event)"
    />
  </main>
</template>

<style scoped>
.message-window {
  height: 100%;
  overflow: hidden;
  background: var(--color-bg);
}
</style>
