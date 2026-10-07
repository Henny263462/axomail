import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

import { isTauri } from "./tauri";

export const MESSAGE_ACTION_EVENT = "axomail://message-action";

export type MessageWindowAction =
  | "reply"
  | "replyAll"
  | "forward"
  | "forwardInline"
  | "forwardAttachment"
  | "archive"
  | "move"
  | "copy"
  | "junk"
  | "delete";

export interface MessageActionPayload {
  action: MessageWindowAction;
  messageId: number;
}

const QUERY_KEY = "message";

export function messageWindowId(search: string = globalThis.location?.search ?? ""): number | null {
  const raw = new URLSearchParams(search).get(QUERY_KEY);
  if (raw === null || !/^\d+$/.test(raw)) return null;
  const id = Number(raw);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

export async function openMessageWindow(messageId: number, title: string): Promise<void> {
  const url = `index.html?${QUERY_KEY}=${messageId}`;
  if (!isTauri()) {
    globalThis.open?.(`?${QUERY_KEY}=${messageId}`, `message-${messageId}`, "width=900,height=720");
    return;
  }
  const label = `message-${messageId}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }
  new WebviewWindow(label, {
    url,
    title,
    width: 900,
    height: 720,
    minWidth: 480,
    minHeight: 360,
    center: true,
  });
}

export async function sendMessageAction(payload: MessageActionPayload): Promise<void> {
  if (!isTauri()) return;
  await emitTo("main", MESSAGE_ACTION_EVENT, payload);
  const main = await WebviewWindow.getByLabel("main");
  await main?.setFocus();
}

export async function closeCurrentWindow(): Promise<void> {
  if (isTauri()) await getCurrentWindow().close();
  else globalThis.close?.();
}

export async function listenMessageActions(
  handler: (payload: MessageActionPayload) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  return listen<MessageActionPayload>(MESSAGE_ACTION_EVENT, (event) => handler(event.payload));
}
