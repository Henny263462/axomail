/**
 * Address-book view models, mirroring the Rust contact DTOs
 * (`apps/desktop/src-tauri/src/dto/contact.rs`) in camelCase. Ids are `number`
 * (guarded to the safe-integer range on the Rust side). Re-exported from
 * `./mail` so `../types/mail` stays the single import surface.
 */

/** One address on a contact. */
export interface ContactEmail {
  email: string;
  /** Presentation label ("home", "work", ...); empty when none. */
  label: string;
  isPrimary: boolean;
}

/** A full contact record for the address-book list and detail views. */
export interface ContactDto {
  id: number;
  displayName: string;
  /** Denormalized primary address, or null for a name-only contact. */
  primaryEmail: string | null;
  note: string;
  /** Every address, primary first. */
  emails: ContactEmail[];
}

/** A contact plus the ids of the groups it belongs to (editor payload). */
export interface ContactDetailDto extends ContactDto {
  groupIds: number[];
}

/** One address in a create/update request. */
export interface ContactEmailInput {
  email: string;
  label: string;
  isPrimary: boolean;
}

/** The create/update payload for a contact. */
export interface ContactInput {
  displayName: string;
  emails: ContactEmailInput[];
  note: string;
}

/** A contact group (mailing list) with its member count. */
export interface ContactGroupDto {
  id: number;
  name: string;
  memberCount: number;
}

/** One recipient-autocomplete suggestion (a contact address or a whole group). */
export interface RecipientSuggestionDto {
  /** `"contact"` or `"group"`. */
  kind: "contact" | "group";
  /** Primary line (contact name or group name). */
  label: string;
  /** Secondary line (the address, or an "N recipients" count). */
  detail: string;
  /** Text inserted into the recipient field (address, or comma-joined group). */
  value: string;
}

/** Outcome of a vCard import: contacts added vs. skipped as already present. */
export interface VCardImportDto {
  imported: number;
  skipped: number;
}
