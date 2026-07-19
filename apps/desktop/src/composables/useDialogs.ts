import { reactive, shallowReactive } from "vue";

/**
 * Controller over the open-state and payload of a fixed set of application
 * dialogs. Each dialog is addressed by a name from the caller-supplied map
 * `M`, whose value type is that dialog's open payload (`undefined` for a
 * dialog that needs none).
 *
 * The state is reactive, so {@link DialogHost} drives its `v-if` mounts from
 * `isOpen(...)` while the host component only opens, closes, and forwards
 * each dialog's domain events. Mounting one further dialog therefore never
 * grows the host: the dialog's markup lives in `DialogHost`, and this
 * controller carries the state.
 */
export interface DialogController<M extends Record<string, unknown>> {
  /** True while `name` is open. Reactive. */
  isOpen(name: keyof M): boolean;
  /** Opens `name`, storing its payload for the host to read. */
  open<K extends keyof M>(name: K, payload?: M[K]): void;
  /** Closes `name` and discards its payload. */
  close(name: keyof M): void;
  /** The payload `name` was opened with, or `null` when closed or absent. */
  payload<K extends keyof M>(name: K): M[K] | null;
}

/** Creates a {@link DialogController} backed by reactive state. */
export function useDialogs<
  M extends Record<string, unknown>,
>(): DialogController<M> {
  const openState = reactive(new Map<keyof M, boolean>());
  // Shallow: payloads are read-only snapshots, so they need reactive
  // set/delete (to re-open with a fresh payload) but no deep value wrapping.
  const payloadState = shallowReactive(new Map<keyof M, M[keyof M]>());

  return {
    isOpen: (name) => openState.get(name) === true,
    open: (name, payload) => {
      payloadState.set(name, payload as M[keyof M]);
      openState.set(name, true);
    },
    close: (name) => {
      openState.set(name, false);
      payloadState.delete(name);
    },
    payload: <K extends keyof M>(name: K): M[K] | null => {
      const value = payloadState.get(name);
      return value === undefined ? null : (value as M[K]);
    },
  };
}
