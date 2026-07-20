/**
 * Two-key chord dispatcher — the "press <kbd>g</kbd>, then <kbd>i</kbd>" prefix
 * shortcuts (go-to navigation) the Stage-0 {@link useCommandRegistry} single
 * dispatcher does not model.
 *
 * A chord is expressed as a whitespace-separated {@link Shortcut} sequence
 * (`"g i"`), so one string is both the registry command's display shortcut and
 * this dispatcher's binding source. The registry's own matcher never fires such
 * a spec (its parsed "key" becomes the literal `"g i"`, which no real
 * `KeyboardEvent.key` equals), so a chord command registered for the palette /
 * help listing dispatches **only** here — no double firing.
 *
 * ## Rules (mirror the registry dispatcher, so both feel identical)
 *
 * - Bubble phase, and an event another handler already `preventDefault`-ed is
 *   ignored: a focused widget's own key map wins.
 * - Chords are bare letter keys; a held Ctrl/Alt/Meta or focus inside a text
 *   input / textarea / select / contenteditable cancels any pending prefix and
 *   never dispatches, so typing is never hijacked.
 * - A prefix key is remembered for {@link DEFAULT_TIMEOUT_MS}; the completing
 *   key within the window runs the matching, enabled chord. A stale prefix, a
 *   non-matching second key, or a disabled binding simply falls through.
 */
import { onScopeDispose } from "vue";

/** A chord binding: the resolved key sequence plus the action to run. */
export interface ChordBinding {
  /** The lowercased key sequence, e.g. `["g", "i"]`. */
  sequence: string[];
  /** Runs the chord. May be async; the dispatcher does not await it. */
  run: () => void | Promise<void>;
  /** When it returns `false`, the chord does not fire (falls through). */
  isEnabled?: () => boolean;
}

/** How long a pressed prefix key waits for its completing key. */
export const DEFAULT_TIMEOUT_MS = 1200;

/**
 * True when `spec` is a multi-key chord sequence (whitespace-separated keys and
 * no `+`), distinguishing `"g i"` from a modifier chord like `"Mod+K"`.
 */
export function isChordSequence(spec: string): boolean {
  return /\s/.test(spec.trim()) && !spec.includes("+");
}

/** Parses `"g i"` into `["g", "i"]` (lowercased); `[]` when not a sequence. */
export function parseChordSequence(spec: string): string[] {
  if (!isChordSequence(spec)) return [];
  return spec
    .trim()
    .split(/\s+/)
    .map((key) => key.toLowerCase());
}

/** Renders a chord for display: `["g", "i"]` → `"G then I"`. */
export function formatChord(sequence: string[]): string {
  return sequence
    .map((key) => (key.length === 1 ? key.toUpperCase() : key))
    .join(" then ");
}

/** The lifecycle-free dispatcher core; unit tests drive it directly. */
export interface ChordDispatcher {
  /** Feeds one keydown; returns whether it completed (and ran) a chord. */
  handleKeydown(event: KeyboardEvent): boolean;
  /** Clears any pending prefix. */
  reset(): void;
}

interface CoreOptions {
  /** The current bindings (read on each keydown so it stays reactive). */
  bindings: () => ChordBinding[];
  /** Prefix lifetime in ms (default {@link DEFAULT_TIMEOUT_MS}). */
  timeoutMs?: number;
  /** Clock source; tests pass a fake for deterministic expiry. */
  now?: () => number;
}

/** Builds a {@link ChordDispatcher} with no framework lifecycle attached. */
export function createChordDispatcher(options: CoreOptions): ChordDispatcher {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const now = options.now ?? (() => Date.now());
  let pending: { key: string; at: number } | null = null;

  function reset(): void {
    pending = null;
  }

  function handleKeydown(event: KeyboardEvent): boolean {
    // A closer handler already claimed it, or an accelerator / typing context:
    // abandon any prefix and never dispatch.
    if (
      event.defaultPrevented ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      isEditableTarget(event.target)
    ) {
      pending = null;
      return false;
    }
    const key = normalizeKey(event.key);
    // A bare modifier keydown (Shift on its own) is not part of a chord and
    // must not expire or reset the pending prefix.
    if (key === "shift" || key === "control" || key === "alt" || key === "meta") {
      return false;
    }
    if (pending !== null && now() - pending.at > timeoutMs) pending = null;

    const bindings = options.bindings();
    if (pending !== null) {
      const prefix = pending.key;
      pending = null;
      const match = bindings.find(
        (binding) =>
          binding.sequence.length === 2 &&
          binding.sequence[0] === prefix &&
          binding.sequence[1] === key,
      );
      if (match !== undefined && (match.isEnabled?.() ?? true)) {
        event.preventDefault();
        void match.run();
        return true;
      }
      // No completion: the current key may itself begin a fresh chord.
    }
    const startsChord = bindings.some(
      (binding) =>
        binding.sequence.length === 2 &&
        binding.sequence[0] === key &&
        (binding.isEnabled?.() ?? true),
    );
    if (startsChord) pending = { key, at: now() };
    return false;
  }

  return { handleKeydown, reset };
}

/**
 * Installs a window-level chord dispatcher for the lifetime of the calling
 * scope. `bindings` is read on every keydown, so a changing set (enabled state)
 * is always current.
 */
export function useKeyChords(
  bindings: () => ChordBinding[],
  options?: { timeoutMs?: number },
): void {
  const dispatcher = createChordDispatcher({
    bindings,
    timeoutMs: options?.timeoutMs,
  });
  const onKeydown = (event: KeyboardEvent): void => {
    dispatcher.handleKeydown(event);
  };
  window.addEventListener("keydown", onKeydown);
  onScopeDispose(() => window.removeEventListener("keydown", onKeydown), true);
}

function normalizeKey(key: string): string {
  return key === " " ? "space" : key.toLowerCase();
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}
