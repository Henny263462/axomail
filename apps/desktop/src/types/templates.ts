/**
 * Message-template view models.
 *
 * Split out of `mail.ts` (size gate); consumed directly via `../types/templates`
 * by the template picker, the composer "save as template" flow, and the api
 * layer. Mirrors the serialized Rust DTO shapes (camelCase).
 */

/** A saved message template, as returned by `list_templates`. */
export interface MessageTemplateDto {
  id: number;
  name: string;
  subject: string;
  to: string[];
  cc: string[];
  bcc: string[];
  bodyText: string;
}

/** Input for `save_template`: the composer's content plus a name. */
export interface NewTemplateInput {
  name: string;
  subject: string;
  to: string[];
  cc: string[];
  bcc: string[];
  bodyText: string;
}
