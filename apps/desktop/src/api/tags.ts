/**
 * Typed wrappers over the tag Tauri commands.
 *
 * Reads use {@link safeInvoke} (resolving to `null` in the browser preview
 * where the IPC bridge is absent, ADR-0005); mutations use
 * {@link invokeOrThrow} so a backend failure reaches the caller's error
 * surface (still resolving to `null` in the browser preview).
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  MessageListPage,
  MessageTagOptions,
  TagDto,
} from "../types/mail";

/** Lists an account's tags with usage counts (tag manager). */
export function listTags(accountId: number): Promise<TagDto[] | null> {
  return safeInvoke<TagDto[]>("list_tags", { accountId });
}

/** Lists every tag across all accounts (sidebar filter). */
export function listAllTags(): Promise<TagDto[] | null> {
  return safeInvoke<TagDto[]>("list_all_tags");
}

/** Creates a tag for an account. Rejects with the backend error string. */
export function createTag(
  accountId: number,
  name: string,
  color: string | null,
): Promise<TagDto | null> {
  return invokeOrThrow<TagDto>("create_tag", { accountId, name, color });
}

/** Updates a tag's display name and color. Rejects on failure. */
export function updateTag(
  tagId: number,
  displayName: string,
  color: string | null,
): Promise<TagDto | null> {
  return invokeOrThrow<TagDto>("update_tag", { tagId, displayName, color });
}

/** Deletes a tag. Rejects with the backend error string on failure. */
export function deleteTag(tagId: number): Promise<null> {
  return invokeOrThrow<null>("delete_tag", { tagId });
}

/** Loads the tag picker options for a message. `null` in browser preview. */
export function messageTagOptions(
  messageId: number,
): Promise<MessageTagOptions | null> {
  return safeInvoke<MessageTagOptions>("message_tag_options", { messageId });
}

/** Applies or removes a tag on a message. Rejects on failure. */
export function setMessageTag(
  messageId: number,
  tagId: number,
  tagged: boolean,
): Promise<null> {
  return invokeOrThrow<null>("set_message_tag", { messageId, tagId, tagged });
}

/** Lists the messages carrying a tag (filter view). `null` in browser preview. */
export function listMessagesForTag(
  tagId: number,
): Promise<MessageListPage | null> {
  return safeInvoke<MessageListPage>("list_messages_for_tag", { tagId });
}
