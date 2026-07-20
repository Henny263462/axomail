/**
 * Typed wrappers over the contact / address-book Tauri commands, plus the
 * dialog-plugin calls that pick a vCard file path (Rust does the disk IO,
 * ADR-0010).
 *
 * Reads use {@link safeInvoke} (resolving to `null` in the browser preview
 * where the IPC bridge is absent, ADR-0005); mutations use {@link invokeOrThrow}
 * so a backend failure reaches the caller's error surface.
 */
import { invokeOrThrow, safeInvoke } from "../lib/tauri";
import type {
  ContactDetailDto,
  ContactDto,
  ContactGroupDto,
  ContactInput,
  RecipientSuggestionDto,
  VCardImportDto,
} from "../types/mail";

/** Lists contacts, optionally filtered by a name/email substring. */
export function listContacts(query = ""): Promise<ContactDto[] | null> {
  return safeInvoke<ContactDto[]>("list_contacts", { query });
}

/** Loads one contact with its group memberships (the editor payload). */
export function getContact(contactId: number): Promise<ContactDetailDto | null> {
  return safeInvoke<ContactDetailDto>("get_contact", { contactId });
}

/** Creates a contact. Rejects with the backend error string on failure. */
export function createContact(input: ContactInput): Promise<ContactDto | null> {
  return invokeOrThrow<ContactDto>("create_contact", { input });
}

/** Updates a contact. Rejects on failure. */
export function updateContact(
  contactId: number,
  input: ContactInput,
): Promise<ContactDto | null> {
  return invokeOrThrow<ContactDto>("update_contact", { contactId, input });
}

/** Deletes a contact. Rejects on failure. */
export function deleteContact(contactId: number): Promise<null> {
  return invokeOrThrow<null>("delete_contact", { contactId });
}

/** Recipient autocomplete suggestions for a typed prefix. */
export function searchRecipients(
  query: string,
  limit?: number,
): Promise<RecipientSuggestionDto[] | null> {
  return safeInvoke<RecipientSuggestionDto[]>("search_recipients", {
    query,
    limit,
  });
}

/** Lists contact groups with their member counts. */
export function listContactGroups(): Promise<ContactGroupDto[] | null> {
  return safeInvoke<ContactGroupDto[]>("list_contact_groups");
}

/** Creates a contact group. Rejects on failure. */
export function createContactGroup(
  name: string,
): Promise<ContactGroupDto | null> {
  return invokeOrThrow<ContactGroupDto>("create_contact_group", { name });
}

/** Deletes a contact group. Rejects on failure. */
export function deleteContactGroup(groupId: number): Promise<null> {
  return invokeOrThrow<null>("delete_contact_group", { groupId });
}

/** Replaces a contact's group memberships. Rejects on failure. */
export function setContactGroups(
  contactId: number,
  groupIds: number[],
): Promise<null> {
  return invokeOrThrow<null>("set_contact_groups", { contactId, groupIds });
}

/** Reads the "collect sent recipients" setting (default true). */
export function getCollectAddresses(): Promise<boolean | null> {
  return safeInvoke<boolean>("get_collect_addresses");
}

/** Sets the "collect sent recipients" setting. Rejects on failure. */
export function setCollectAddresses(enabled: boolean): Promise<null> {
  return invokeOrThrow<null>("set_collect_addresses", { enabled });
}

/**
 * Opens the native open-file dialog for a `.vcf` file and imports it. Resolves
 * to the import outcome, or `null` when the user cancels or the desktop bridge
 * is unavailable.
 */
export async function importContactsVcard(): Promise<VCardImportDto | null> {
  const path = await safeInvoke<string | null>("plugin:dialog|open", {
    options: {
      multiple: false,
      directory: false,
      title: "Import contacts",
      filters: [{ name: "vCard", extensions: ["vcf", "vcard"] }],
    },
  });
  if (path === null || path === undefined) return null;
  return invokeOrThrow<VCardImportDto>("import_contacts_vcard", { path });
}

/**
 * Opens the native save-file dialog and exports the whole address book as a
 * vCard document. Resolves to the number of contacts written, or `null` on
 * cancel / absent bridge.
 */
export async function exportContactsVcard(
  version: "3.0" | "4.0" = "3.0",
): Promise<number | null> {
  const path = await safeInvoke<string | null>("plugin:dialog|save", {
    options: {
      defaultPath: "contacts.vcf",
      filters: [{ name: "vCard", extensions: ["vcf"] }],
    },
  });
  if (path === null || path === undefined) return null;
  return invokeOrThrow<number>("export_contacts_vcard", { path, version });
}
