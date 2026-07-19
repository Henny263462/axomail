import { computed, ref, type Ref } from "vue";

import {
  cancelScheduledSend,
  deleteDraft,
  listDrafts,
  listOutbox,
  retryOutbox,
  sendOutboxNow,
} from "../api/compose";
import type { AppDialogMap } from "../components/layout/dialogs";
import type { SidebarView } from "../components/sidebar/view";
import type { AccountSummary, DraftDto, OutboxItemDto } from "../types/mail";
import type { DialogController } from "./useDialogs";

/**
 * The App-level state the compose workflow reads and drives. All are the
 * reactive refs `App.vue` already owns (props-down/events-up; no store).
 */
interface Deps {
  /** The active sidebar view; the Drafts/Outbox entries switch it. */
  activeView: Ref<SidebarView>;
  /** Accounts, used to pick the account a brand-new message composes from. */
  accounts: Ref<AccountSummary[]>;
  /** The selected mailbox, used to prefer its account for a new message. */
  selectedMailboxId: Ref<number | null>;
  /** The open message; cleared when switching to the Drafts/Outbox lists. */
  selectedMessageId: Ref<number | null>;
  /** Controller the composer dialog is opened through. */
  dialogs: DialogController<AppDialogMap>;
  /** Reveals the list column on the narrow layout (`shell.showPane("list")`). */
  showList: () => void;
}

/**
 * Owns everything about composing, drafts, and the outbox: the two lists, the
 * Drafts/Outbox view entries, opening the composer (blank or from a draft), and
 * the composer/outbox event handlers. Extracted from `App.vue` so the Wave 3
 * compose tracks (reply/forward entry, rich compose) extend one cohesive seam
 * instead of the App shell. Behaviour is identical to the previous inline code.
 */
export function useComposeWorkflow(deps: Deps) {
  const drafts = ref<DraftDto[]>([]);
  const outbox = ref<OutboxItemDto[]>([]);
  const selectedDraftId = ref<number | null>(null);

  /** Account a new message is composed from: the selected one, else the first. */
  const composeAccountId = computed<number | null>(() => {
    const owning = deps.accounts.value.find((account) =>
      account.mailboxes.some(
        (mailbox) => mailbox.id === deps.selectedMailboxId.value,
      ),
    );
    return owning?.id ?? deps.accounts.value[0]?.id ?? null;
  });

  async function loadDrafts(): Promise<void> {
    drafts.value = (await listDrafts()) ?? [];
  }

  async function loadOutbox(): Promise<void> {
    outbox.value = (await listOutbox()) ?? [];
  }

  async function selectDrafts(): Promise<void> {
    deps.activeView.value = "drafts";
    deps.selectedMessageId.value = null;
    deps.showList();
    await loadDrafts();
  }

  async function selectOutbox(): Promise<void> {
    deps.activeView.value = "outbox";
    deps.selectedMessageId.value = null;
    deps.showList();
    await loadOutbox();
  }

  function openCompose(): void {
    const id = composeAccountId.value;
    if (id === null) return;
    deps.dialogs.open("composer", { accountId: id, draft: null });
  }

  function openDraft(draftId: number): void {
    const draft = drafts.value.find((d) => d.id === draftId);
    if (draft === undefined) return;
    selectedDraftId.value = draftId;
    deps.dialogs.open("composer", { accountId: draft.accountId, draft });
  }

  async function onComposerSent(payload: {
    draftId: number | null;
  }): Promise<void> {
    deps.dialogs.close("composer");
    // A draft that was sent is removed client-side (send then delete).
    if (payload.draftId !== null) await deleteDraft(payload.draftId);
    if (deps.activeView.value === "drafts") await loadDrafts();
    if (deps.activeView.value === "outbox") await loadOutbox();
  }

  async function onComposerSaved(): Promise<void> {
    if (deps.activeView.value === "drafts") await loadDrafts();
  }

  async function onRetryOutbox(outboxId: number): Promise<void> {
    await retryOutbox(outboxId);
    await loadOutbox();
  }

  async function onSendOutboxNow(outboxId: number): Promise<void> {
    await sendOutboxNow(outboxId);
    await loadOutbox();
  }

  async function onCancelSchedule(outboxId: number): Promise<void> {
    await cancelScheduledSend(outboxId);
    // The message becomes a draft, so both lists may have changed.
    await loadOutbox();
    if (deps.activeView.value === "drafts") await loadDrafts();
  }

  return {
    drafts,
    outbox,
    selectedDraftId,
    loadDrafts,
    loadOutbox,
    selectDrafts,
    selectOutbox,
    openCompose,
    openDraft,
    onComposerSent,
    onComposerSaved,
    onRetryOutbox,
    onSendOutboxNow,
    onCancelSchedule,
  };
}
