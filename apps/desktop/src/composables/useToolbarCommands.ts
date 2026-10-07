import { onScopeDispose, type ComputedRef, type Ref } from "vue";

import { syncAccount } from "../api/sync";
import { i18n } from "../i18n";
import {
  listenMessageActions,
  openMessageWindow,
  type MessageActionPayload,
} from "../lib/messageWindow";
import type { ForwardStyle } from "../types/compose";
import type { AccountSummary } from "../types/mail";
import type { CommandRegistry } from "./useCommandRegistry";

export interface ToolbarCommandDeps {
  isModalOpen: () => boolean;
  accounts: Ref<AccountSummary[]>;
  openMessageId: ComputedRef<number | null>;
  openSettings: () => void;
  reply: (id: number) => void;
  replyAll: (id: number) => void;
  forward: (id: number, style?: ForwardStyle) => void;
  archive: (id: number) => void;
  move: (id: number) => void;
  copy: (id: number) => void;
  junk: (id: number) => void;
  remove: (id: number) => void;
}

function label(key: string): string {
  return i18n.global.t(`toolbar.${key}`);
}

export async function syncAllAccounts(accounts: readonly AccountSummary[]): Promise<void> {
  await Promise.all(accounts.map((account) => syncAccount(account.id)));
}

export function useToolbarCommands(registry: CommandRegistry, deps: ToolbarCommandDeps): void {
  const notBusy = (): boolean => !deps.isModalOpen();
  const hasMessage = (): boolean => notBusy() && deps.openMessageId.value !== null;
  const onMessage = (action: (id: number) => void) => (): void => {
    const id = deps.openMessageId.value;
    if (id !== null) action(id);
  };

  registry.registerAll([
    {
      id: "mail.getAll",
      get title() {
        return label("getMail");
      },
      keywords: ["sync", "fetch", "get", "receive", "abrufen"],
      shortcut: "F5",
      run: () => syncAllAccounts(deps.accounts.value),
      isEnabled: () => notBusy() && deps.accounts.value.length > 0,
    },
    {
      id: "app.settings",
      get title() {
        return label("settings");
      },
      keywords: ["settings", "preferences", "options", "einstellungen"],
      shortcut: "Mod+,",
      run: deps.openSettings,
      isEnabled: notBusy,
    },
    {
      id: "message.forwardInline",
      get title() {
        return label("forwardInline");
      },
      keywords: ["forward", "inline"],
      run: onMessage((id) => deps.forward(id, "inline")),
      isEnabled: hasMessage,
    },
    {
      id: "message.forwardAttachment",
      get title() {
        return label("forwardAttachment");
      },
      keywords: ["forward", "attachment", "eml"],
      run: onMessage((id) => deps.forward(id, "attachment")),
      isEnabled: hasMessage,
    },
    {
      id: "message.move",
      get title() {
        return label("move");
      },
      keywords: ["move", "folder"],
      shortcut: "m",
      run: onMessage(deps.move),
      isEnabled: hasMessage,
    },
    {
      id: "message.copy",
      get title() {
        return label("copy");
      },
      keywords: ["copy", "folder"],
      run: onMessage(deps.copy),
      isEnabled: hasMessage,
    },
    {
      id: "message.openWindow",
      get title() {
        return label("openWindow");
      },
      keywords: ["window", "open", "separate", "fenster"],
      shortcut: "o",
      run: onMessage((id) => void openMessageWindow(id, label("windowTitle"))),
      isEnabled: hasMessage,
    },
  ]);

  const handlers: Record<MessageActionPayload["action"], (id: number) => void> = {
    reply: deps.reply,
    replyAll: deps.replyAll,
    forward: (id) => deps.forward(id),
    forwardInline: (id) => deps.forward(id, "inline"),
    forwardAttachment: (id) => deps.forward(id, "attachment"),
    archive: deps.archive,
    move: deps.move,
    copy: deps.copy,
    junk: deps.junk,
    delete: deps.remove,
  };

  let disposed = false;
  let unlisten: (() => void) | null = null;
  void listenMessageActions((payload) => handlers[payload.action]?.(payload.messageId)).then((off) => {
    if (disposed) off?.();
    else unlisten = off;
  });
  onScopeDispose(() => {
    disposed = true;
    unlisten?.();
  });
}
