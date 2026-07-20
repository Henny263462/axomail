import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";

import {
  createContact,
  getCollectAddresses,
  getContact,
  importContactsVcard,
  listContactGroups,
  listContacts,
  searchRecipients,
  setCollectAddresses,
  setContactGroups,
} from "../src/api/contacts";
import RecipientInput from "../src/components/compose/RecipientInput.vue";
import AddressBookDialog from "../src/components/contacts/AddressBookDialog.vue";
import ContactEditor from "../src/components/contacts/ContactEditor.vue";
import ContactsSection from "../src/components/settings/ContactsSection.vue";
import type { SettingsSectionContext } from "../src/components/settings/context";
import type {
  ContactDetailDto,
  ContactDto,
  RecipientSuggestionDto,
} from "../src/types/mail";

vi.mock("../src/api/contacts", () => ({
  listContacts: vi.fn(),
  getContact: vi.fn(),
  createContact: vi.fn(),
  updateContact: vi.fn(),
  deleteContact: vi.fn(),
  searchRecipients: vi.fn(),
  listContactGroups: vi.fn(),
  createContactGroup: vi.fn(),
  deleteContactGroup: vi.fn(),
  setContactGroups: vi.fn(),
  getCollectAddresses: vi.fn(),
  setCollectAddresses: vi.fn(),
  importContactsVcard: vi.fn(),
  exportContactsVcard: vi.fn(),
}));

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const searchMock = vi.mocked(searchRecipients);
const listMock = vi.mocked(listContacts);
const getMock = vi.mocked(getContact);
const createMock = vi.mocked(createContact);
const setGroupsMock = vi.mocked(setContactGroups);
const groupsMock = vi.mocked(listContactGroups);

function contact(id: number, name: string, email: string): ContactDto {
  return {
    id,
    displayName: name,
    primaryEmail: email,
    note: "",
    emails: [{ email, label: "", isPrimary: true }],
  };
}

function suggestion(
  kind: "contact" | "group",
  label: string,
  value: string,
): RecipientSuggestionDto {
  return { kind, label, detail: value, value };
}

const tick = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.clearAllMocks();
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
  listMock.mockResolvedValue([]);
  groupsMock.mockResolvedValue([]);
  vi.mocked(getCollectAddresses).mockResolvedValue(true);
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("RecipientInput autocomplete", () => {
  it("searches the last token and shows suggestions", async () => {
    searchMock.mockResolvedValue([suggestion("contact", "Ada Lovelace", "ada@example.org")]);
    const wrapper = mount(RecipientInput, {
      props: { fieldId: "to", label: "To", debounceMs: 0 },
    });
    await wrapper.find("input").setValue("ad");
    await tick();
    await flushPromises();

    expect(searchMock).toHaveBeenCalledWith("ad", 8);
    const options = wrapper.findAll("[role=option]");
    expect(options).toHaveLength(1);
    expect(options[0]?.text()).toContain("Ada Lovelace");
  });

  it("inserts the address on Enter after arrow navigation", async () => {
    searchMock.mockResolvedValue([
      suggestion("contact", "Ada", "ada@example.org"),
      suggestion("contact", "Grace", "grace@example.org"),
    ]);
    const wrapper = mount(RecipientInput, {
      props: { fieldId: "to", label: "To", debounceMs: 0 },
    });
    const input = wrapper.find("input");
    await input.setValue("a");
    await tick();
    await flushPromises();

    await input.trigger("keydown", { key: "ArrowDown" });
    await input.trigger("keydown", { key: "Enter" });

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted?.at(-1)).toEqual(["grace@example.org, "]);
  });

  it("keeps earlier recipients when completing a later token", async () => {
    searchMock.mockResolvedValue([suggestion("contact", "Grace", "grace@example.org")]);
    const wrapper = mount(RecipientInput, {
      props: { fieldId: "to", label: "To", debounceMs: 0, modelValue: "ada@example.org, gr" },
    });
    await wrapper.find("input").trigger("input");
    await tick();
    await flushPromises();
    await wrapper.find("[role=option]").trigger("mousedown");

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted?.at(-1)).toEqual(["ada@example.org, grace@example.org, "]);
  });

  it("expands a group suggestion to its members", async () => {
    searchMock.mockResolvedValue([
      suggestion("group", "Team", "a@example.org, b@example.org"),
    ]);
    const wrapper = mount(RecipientInput, {
      props: { fieldId: "cc", label: "Cc", debounceMs: 0 },
    });
    await wrapper.find("input").setValue("Tea");
    await tick();
    await flushPromises();
    expect(wrapper.find(".badge").exists()).toBe(true);
    await wrapper.find("[role=option]").trigger("mousedown");
    expect(wrapper.emitted("update:modelValue")?.at(-1)).toEqual([
      "a@example.org, b@example.org, ",
    ]);
  });

  it("does not query for an empty token", async () => {
    const wrapper = mount(RecipientInput, {
      props: { fieldId: "to", label: "To", debounceMs: 0 },
    });
    await wrapper.find("input").setValue("");
    await tick();
    expect(searchMock).not.toHaveBeenCalled();
    expect(wrapper.find("[role=listbox]").exists()).toBe(false);
  });

  it("cancels a pending debounced search on blur (no late reopen)", async () => {
    vi.useFakeTimers();
    try {
      searchMock.mockResolvedValue([suggestion("contact", "Ann", "ann@example.org")]);
      const wrapper = mount(RecipientInput, {
        props: { fieldId: "to", label: "To", debounceMs: 140 },
      });
      const input = wrapper.find("input");
      await input.setValue("ann");
      // The debounce is still pending when the field loses focus.
      await input.trigger("blur");
      // Let the (now-cancelled) debounce window elapse.
      vi.advanceTimersByTime(300);
      await flushPromises();

      // Without clearing the timer on close, the stale search would fire and
      // reopen the dropdown after the field moved on.
      expect(searchMock).not.toHaveBeenCalled();
      expect(wrapper.find("[role=listbox]").exists()).toBe(false);
      wrapper.unmount();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("ContactEditor", () => {
  it("emits a validated input with a single primary on save", async () => {
    const wrapper = mount(ContactEditor, {
      props: { contact: null, groups: [] },
    });
    const inputs = wrapper.findAll("input[type=email]");
    await inputs[0]?.setValue("ada@example.org");
    await wrapper.find("input#contact-name").setValue("Ada");
    await wrapper.find("form").trigger("submit");

    const emitted = wrapper.emitted("save");
    expect(emitted).toHaveLength(1);
    const [input, groupIds, id] = emitted![0] as unknown[];
    expect(id).toBeNull();
    expect(groupIds).toEqual([]);
    expect(input).toMatchObject({
      displayName: "Ada",
      emails: [{ email: "ada@example.org", isPrimary: true }],
    });
  });

  it("blocks an empty contact with an alert", async () => {
    const wrapper = mount(ContactEditor, { props: { contact: null, groups: [] } });
    await wrapper.find("form").trigger("submit");
    expect(wrapper.emitted("save")).toBeUndefined();
    expect(wrapper.find("[role=alert]").exists()).toBe(true);
  });
});

describe("AddressBookDialog", () => {
  it("lists contacts and shows an honest empty editor hint", async () => {
    listMock.mockResolvedValue([contact(1, "Ada Lovelace", "ada@example.org")]);
    const wrapper = mount(AddressBookDialog);
    await flushPromises();

    expect(wrapper.text()).toContain("Ada Lovelace");
    expect(wrapper.text()).toContain("Select a contact to edit");
    // The dialog is a labelled modal.
    expect(wrapper.find("[role=dialog][aria-modal=true]").exists()).toBe(true);
  });

  it("creates a contact and assigns its groups", async () => {
    listMock.mockResolvedValue([]);
    groupsMock.mockResolvedValue([]);
    createMock.mockResolvedValue(contact(7, "Grace", "grace@example.org"));
    getMock.mockResolvedValue({
      ...contact(7, "Grace", "grace@example.org"),
      groupIds: [],
    } as ContactDetailDto);
    setGroupsMock.mockResolvedValue(null);

    const wrapper = mount(AddressBookDialog);
    await flushPromises();

    await wrapper.get(".new-button").trigger("click");
    await wrapper.get("input#contact-name").setValue("Grace");
    await wrapper.get("input[type=email]").setValue("grace@example.org");
    await wrapper.get("form.editor").trigger("submit");
    await flushPromises();

    expect(createMock).toHaveBeenCalledTimes(1);
    expect(setGroupsMock).toHaveBeenCalledWith(7, []);
    expect(wrapper.text()).toContain("Saved");
  });

  it("imports a vCard and reports the outcome", async () => {
    vi.mocked(importContactsVcard).mockResolvedValue({ imported: 3, skipped: 1 });
    const wrapper = mount(AddressBookDialog);
    await flushPromises();
    await wrapper.get("button.button").trigger("click"); // "Import vCard" is first
    await flushPromises();
    expect(importContactsVcard).toHaveBeenCalled();
    expect(wrapper.text()).toContain("Imported 3");
  });

  it("moves focus into the dialog on open so Escape reaches its trap", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    listMock.mockResolvedValue([]);

    const wrapper = mount(AddressBookDialog, { attachTo: document.body });
    await flushPromises();
    await nextTick();

    // Focus was pulled inside the dialog (otherwise the overlay keydown trap
    // never fires and Escape can't close it).
    const dialog = wrapper.get("[role=dialog]").element;
    expect(dialog.contains(document.activeElement)).toBe(true);

    await wrapper.get(".overlay").trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("close")).toBeTruthy();

    // Focus returns to the invoker on close.
    wrapper.unmount();
    expect(document.activeElement).toBe(trigger);
  });
});

describe("ContactsSection", () => {
  function fakeContext(): SettingsSectionContext {
    return {
      accounts: [],
      initialPolicies: [],
      notificationsEnabled: true,
      syncIntervalMinutes: 10,
      theme: "system",
      autoMarkReadEnabled: true,
      autoMarkReadDelayMs: 0,
      setNotificationsEnabled: vi.fn(),
      setSyncIntervalMinutes: vi.fn(),
      setAutoMarkRead: vi.fn(),
      setTheme: vi.fn(),
      reportError: vi.fn(),
      reportStatus: vi.fn(),
      notifyAccountsChanged: vi.fn(),
      notifyTagsChanged: vi.fn(),
    };
  }

  it("loads the current value and persists a toggle", async () => {
    vi.mocked(getCollectAddresses).mockResolvedValue(true);
    vi.mocked(setCollectAddresses).mockResolvedValue(null);
    const context = fakeContext();
    const wrapper = mount(ContactsSection, { props: { context } });
    await flushPromises();

    const box = wrapper.find("input[type=checkbox]");
    expect((box.element as HTMLInputElement).checked).toBe(true);

    await box.setValue(false);
    await flushPromises();
    expect(setCollectAddresses).toHaveBeenCalledWith(false);
    expect(context.reportStatus).toHaveBeenCalled();
  });
});
