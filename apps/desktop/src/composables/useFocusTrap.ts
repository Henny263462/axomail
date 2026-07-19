/**
 * Keyboard focus trap for modal dialogs.
 *
 * Takes the dialog container ref (attach it via `ref="root"`) and a `close`
 * callback, and returns an `onKeydown` handler to bind on the container.
 * Escape invokes `close`; Tab / Shift+Tab cycle focus within the dialog's
 * focusable controls so it never escapes to the page behind the modal.
 */
import type { Ref } from "vue";

export interface FocusTrap {
  /** Bind on the container via `@keydown`. */
  onKeydown: (event: KeyboardEvent) => void;
}

export function useFocusTrap(
  root: Ref<HTMLElement | null>,
  close: () => void,
): FocusTrap {
  function focusables(): HTMLElement[] {
    if (root.value === null) return [];
    return Array.from(
      root.value.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled])",
      ),
    );
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusables();
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }

  return { onKeydown };
}
