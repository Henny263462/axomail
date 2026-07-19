import { mount, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

import DeleteAccountDialog from "../src/components/settings/DeleteAccountDialog.vue";

const EMAIL = "klaus@theferbers.eu";

function mountDialog(): VueWrapper {
  return mount(DeleteAccountDialog, {
    props: {
      displayName: "Ferber Mail",
      email: EMAIL,
      deleting: false,
      errorMessage: "",
    },
    attachTo: document.body,
  });
}

describe("DeleteAccountDialog focus containment", () => {
  it("moves focus into the alertdialog on open", () => {
    const wrapper = mountDialog();
    expect(document.activeElement).toBe(
      wrapper.get("#delete-account-confirm").element,
    );
    wrapper.unmount();
  });

  it("traps Tab and Shift+Tab within the alertdialog", async () => {
    const wrapper = mountDialog();
    // Enable the Delete button so it joins the focusable cycle.
    await wrapper.get("#delete-account-confirm").setValue(EMAIL);

    const input = wrapper.get("#delete-account-confirm").element as HTMLElement;
    const del = wrapper.get("button.danger").element as HTMLElement;

    // Tab off the last focusable (Delete) wraps back to the first (input).
    del.focus();
    await wrapper.get(".overlay").trigger("keydown", { key: "Tab" });
    expect(document.activeElement).toBe(input);

    // Shift+Tab off the first focusable (input) wraps to the last (Delete).
    input.focus();
    await wrapper
      .get(".overlay")
      .trigger("keydown", { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(del);
    wrapper.unmount();
  });

  it("cancels on Escape without letting it reach the dialog behind", async () => {
    const ancestorSaw = vi.fn();
    document.body.addEventListener("keydown", ancestorSaw);
    const wrapper = mountDialog();

    await wrapper.get(".overlay").trigger("keydown", { key: "Escape" });

    expect(wrapper.emitted("cancel")).toHaveLength(1);
    // Escape must not bubble to the settings dialog that hosts this one.
    expect(ancestorSaw).not.toHaveBeenCalled();

    document.body.removeEventListener("keydown", ancestorSaw);
    wrapper.unmount();
  });

  it("restores focus to the invoker when it closes", async () => {
    const invoker = document.createElement("button");
    document.body.appendChild(invoker);
    invoker.focus();

    const wrapper = mountDialog();
    // Opening moved focus into the dialog.
    expect(document.activeElement).toBe(
      wrapper.get("#delete-account-confirm").element,
    );

    wrapper.unmount();
    // Closing returns focus to whatever opened the dialog.
    expect(document.activeElement).toBe(invoker);
    invoker.remove();
  });
});
