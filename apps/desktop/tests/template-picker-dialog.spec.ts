import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import TemplatePickerDialog from "../src/components/compose/TemplatePickerDialog.vue";
import { isTauri } from "../src/lib/tauri";

vi.mock("../src/api/templates", () => ({
  listTemplates: vi.fn(),
  deleteTemplate: vi.fn(),
}));
vi.mock("../src/lib/tauri", () => ({ isTauri: vi.fn() }));

const isTauriMock = vi.mocked(isTauri);

beforeEach(() => {
  vi.clearAllMocks();
  // Browser-preview path: the dialog shows its unavailable notice and mounts
  // without a backend, which is all the focus test needs.
  isTauriMock.mockReturnValue(false);
});

afterEach(() => {
  document.body.innerHTML = "";
});

// Finding 11: the modal moves focus into itself on open and restores it on close.
describe("TemplatePickerDialog focus management", () => {
  it("moves focus into the dialog on open and restores it on close", async () => {
    const invoker = document.createElement("button");
    document.body.appendChild(invoker);
    invoker.focus();
    expect(document.activeElement).toBe(invoker);

    const wrapper = mount(TemplatePickerDialog, { attachTo: document.body });
    await flushPromises();

    const dialog = wrapper.get('[role="dialog"]').element;
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(invoker);

    wrapper.unmount();
    // Focus returns to the control that opened the dialog.
    expect(document.activeElement).toBe(invoker);
  });

  it("closes on Escape through the focus trap", async () => {
    const wrapper = mount(TemplatePickerDialog, { attachTo: document.body });
    await flushPromises();

    await wrapper.get(".overlay").trigger("keydown", { key: "Escape" });
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });
});
