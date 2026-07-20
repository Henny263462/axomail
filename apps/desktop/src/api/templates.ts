/**
 * Typed wrappers over the message-template Tauri commands.
 *
 * Each call resolves to its DTO in the desktop shell, or to `null` in the
 * browser preview where the IPC bridge is absent (ADR-0005).
 */
import { safeInvoke } from "../lib/tauri";
import type { MessageTemplateDto, NewTemplateInput } from "../types/templates";

/** Lists every saved template. Resolves to `null` in the browser preview. */
export function listTemplates(): Promise<MessageTemplateDto[] | null> {
  return safeInvoke<MessageTemplateDto[]>("list_templates");
}

/**
 * Saves the composer's current content as a named template, resolving to the
 * stored template (with its new id), or `null` in the browser preview.
 */
export function saveTemplate(
  input: NewTemplateInput,
): Promise<MessageTemplateDto | null> {
  return safeInvoke<MessageTemplateDto>("save_template", { input });
}

/** Deletes a saved template. Resolves to `null` in the browser preview. */
export function deleteTemplate(templateId: number): Promise<null> {
  return safeInvoke<null>("delete_template", { templateId });
}
