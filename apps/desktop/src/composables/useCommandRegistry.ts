/**
 * Command registry — the dependency-free foundation Wave 4 builds on.
 *
 * Features register {@link Command}s (an id, title, keywords, an optional
 * shortcut, and `run`); the palette (track 4.5) reads the reactive
 * {@link CommandRegistry.commands} list, and a single global keydown dispatcher
 * runs the command whose shortcut matches the chord. No store, no dependencies —
 * just Vue reactivity and one `window` listener installed at the app root.
 *
 * ## How the pieces fit
 *
 * - `provideCommandRegistry()` is called **once** at the app root (`App.vue`).
 *   It creates the registry, installs the global keydown dispatcher, and
 *   provides the registry to every descendant. Disposed on unmount.
 * - `useCommandRegistry()` is called by any descendant: the palette reads
 *   `commands`; a feature calls `register(...)` / `registerAll(...)`, whose
 *   registrations auto-unregister when that component unmounts.
 * - `createCommandRegistry()` is the lifecycle-free core (used by the two above
 *   and directly by unit tests).
 *
 * ## Dispatcher rules
 *
 * - The listener is on the bubble phase and skips an event that a closer
 *   handler already `preventDefault`-ed, so a focused widget's own key map (the
 *   message listbox's arrows / Enter / Delete / `a`) always wins over a global
 *   command bound to the same key.
 * - It never hijacks typing: when focus is in a text input, textarea, select,
 *   or contenteditable, only chords carrying a Ctrl/Alt/Meta modifier dispatch
 *   (e.g. `Mod+K` still opens the palette); a bare or Shift-only key falls
 *   through to the field.
 * - A matched command whose `isEnabled()` is `false` does not fire; the
 *   keystroke falls through.
 */
import {
  computed,
  inject,
  onScopeDispose,
  onUnmounted,
  provide,
  shallowReactive,
  type ComputedRef,
  type InjectionKey,
} from "vue";

import type { Command, Shortcut } from "../types/command";

export type { Command, Shortcut } from "../types/command";

/** The registry surface shared across the app. */
export interface CommandRegistry {
  /** Registered commands in registration order (reactive). */
  readonly commands: ComputedRef<Command[]>;
  /** Registers `command`, replacing any with the same id; returns an unregister. */
  register(command: Command): () => void;
  /** Registers several at once; the returned fn unregisters them all. */
  registerAll(commands: Command[]): () => void;
  /** Removes the command with `id`, if present. */
  unregister(id: string): void;
  /** Looks a command up by id. */
  get(id: string): Command | undefined;
  /**
   * Dispatches a keydown to the first matching, enabled command. Returns whether
   * a command was run. Exposed for the app root's listener and for tests.
   */
  handleKeydown(event: KeyboardEvent): boolean;
}

interface Entry {
  command: Command;
  parsed: ParsedShortcut | null;
}

const REGISTRY_KEY: InjectionKey<CommandRegistry> = Symbol("commandRegistry");

/** Creates a registry with no framework lifecycle attached. */
export function createCommandRegistry(): CommandRegistry {
  // shallowReactive so add/delete are tracked for `commands`, but the stored
  // Command objects stay raw (identity-stable for the palette; no deep proxy).
  const entries = shallowReactive(new Map<string, Entry>());

  const commands = computed(() =>
    Array.from(entries.values(), (entry) => entry.command),
  );

  function register(command: Command): () => void {
    const entry: Entry = {
      command,
      parsed: command.shortcut ? parseShortcut(command.shortcut) : null,
    };
    entries.set(command.id, entry);
    return () => {
      // Only remove if this exact registration is still current: a later
      // re-register of the same id must not be dropped by a stale disposer.
      if (entries.get(command.id) === entry) entries.delete(command.id);
    };
  }

  function registerAll(list: Command[]): () => void {
    const offs = list.map(register);
    return () => {
      for (const off of offs) off();
    };
  }

  function handleKeydown(event: KeyboardEvent): boolean {
    if (event.defaultPrevented) return false;
    const editable = isEditableTarget(event.target);
    for (const entry of entries.values()) {
      if (entry.parsed === null || !matchesShortcut(entry.parsed, event)) {
        continue;
      }
      // In a text field, never intercept what is (or could become) typing:
      // only a Ctrl/Alt/Meta chord is allowed through; a bare/Shift key falls
      // to the field. Return, not continue — the keystroke belongs to it.
      if (editable && !hasNonShiftModifier(entry.parsed)) return false;
      if (entry.command.isEnabled && !entry.command.isEnabled()) continue;
      event.preventDefault();
      void entry.command.run();
      return true;
    }
    return false;
  }

  return {
    commands,
    register,
    registerAll,
    unregister: (id) => {
      entries.delete(id);
    },
    get: (id) => entries.get(id)?.command,
    handleKeydown,
  };
}

/**
 * Creates the app-wide registry, installs the global keydown dispatcher, and
 * provides it to descendants. Call once, at the app root. Returns the registry
 * so the root may also register its own commands.
 */
export function provideCommandRegistry(): CommandRegistry {
  const registry = createCommandRegistry();
  const onKeydown = (event: KeyboardEvent): void => {
    registry.handleKeydown(event);
  };
  // Bubble phase: a focused widget's own keydown handler runs first and can
  // claim the event via preventDefault before it reaches this listener.
  window.addEventListener("keydown", onKeydown);
  onUnmounted(() => window.removeEventListener("keydown", onKeydown));
  provide(REGISTRY_KEY, registry);
  return registry;
}

/** Scoped view of the registry: registrations auto-dispose on unmount. */
export interface ScopedCommandRegistry {
  /** The reactive command list (for the palette / help overlay). */
  commands: ComputedRef<Command[]>;
  /** Registers a command for this component's lifetime. */
  register(command: Command): () => void;
  /** Registers several commands for this component's lifetime. */
  registerAll(commands: Command[]): () => void;
  /** Looks a command up by id. */
  get(id: string): Command | undefined;
}

/**
 * Injects the app-wide registry. Any command registered through the returned
 * `register` / `registerAll` is automatically unregistered when the calling
 * component unmounts, so features never leak commands. Throws if no
 * {@link provideCommandRegistry} ancestor exists.
 */
export function useCommandRegistry(): ScopedCommandRegistry {
  const registry = inject(REGISTRY_KEY, null);
  if (registry === null) {
    throw new Error(
      "useCommandRegistry() requires provideCommandRegistry() in an ancestor (call it at the app root).",
    );
  }
  const disposers: (() => void)[] = [];
  function track(off: () => void): () => void {
    disposers.push(off);
    return off;
  }
  // Fail-silently so a scope-less unit call does not warn; mounted components
  // dispose their registrations here.
  onScopeDispose(() => {
    for (const off of disposers.splice(0)) off();
  }, true);

  return {
    commands: registry.commands,
    register: (command) => track(registry.register(command)),
    registerAll: (commands) => track(registry.registerAll(commands)),
    get: registry.get,
  };
}

// --- shortcut parsing / matching ------------------------------------------

interface ParsedShortcut {
  /** Normalized key (lowercased `KeyboardEvent.key`; space is `"space"`). */
  key: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  /** Platform-primary accelerator (`Meta` on macOS, `Ctrl` elsewhere). */
  mod: boolean;
}

type ModifierFlag = "ctrl" | "alt" | "shift" | "meta" | "mod";

const MODIFIER_TOKENS: Record<string, ModifierFlag> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  meta: "meta",
  cmd: "meta",
  command: "meta",
  super: "meta",
  win: "meta",
  mod: "mod",
};

/** Parses a {@link Shortcut} spec; returns null when it names no key. */
export function parseShortcut(spec: Shortcut): ParsedShortcut | null {
  const parsed: ParsedShortcut = {
    key: "",
    ctrl: false,
    alt: false,
    shift: false,
    meta: false,
    mod: false,
  };
  let keySet = false;
  for (const raw of spec.split("+")) {
    const token = raw.trim().toLowerCase();
    if (token === "") continue;
    const modifier = MODIFIER_TOKENS[token];
    if (modifier !== undefined) {
      parsed[modifier] = true;
    } else {
      parsed.key = token === "space" ? "space" : token;
      keySet = true;
    }
  }
  return keySet ? parsed : null;
}

function normalizeEventKey(key: string): string {
  return key === " " ? "space" : key.toLowerCase();
}

function matchesShortcut(parsed: ParsedShortcut, event: KeyboardEvent): boolean {
  if (normalizeEventKey(event.key) !== parsed.key) return false;
  const mac = isMac();
  const wantCtrl = parsed.ctrl || (parsed.mod && !mac);
  const wantMeta = parsed.meta || (parsed.mod && mac);
  return (
    event.ctrlKey === wantCtrl &&
    event.metaKey === wantMeta &&
    event.altKey === parsed.alt &&
    event.shiftKey === parsed.shift
  );
}

function hasNonShiftModifier(parsed: ParsedShortcut): boolean {
  return parsed.ctrl || parsed.alt || parsed.meta || parsed.mod;
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

function isMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const source = `${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`;
  return /mac|iphone|ipad|ipod/i.test(source);
}

/**
 * Renders a {@link Shortcut} for display (palette / help overlay): normalized
 * order, platform glyphs on macOS (`⌘⌥⇧⌃`) and word labels elsewhere. Returns
 * an empty string for a spec that names no key.
 */
export function formatShortcut(spec: Shortcut): string {
  const parsed = parseShortcut(spec);
  if (parsed === null) return "";
  const mac = isMac();
  const parts: string[] = [];
  if (parsed.mod) parts.push(mac ? "⌘" : "Ctrl");
  if (parsed.ctrl) parts.push(mac ? "⌃" : "Ctrl");
  if (parsed.alt) parts.push(mac ? "⌥" : "Alt");
  if (parsed.shift) parts.push(mac ? "⇧" : "Shift");
  if (parsed.meta) parts.push(mac ? "⌘" : "Meta");
  parts.push(displayKey(parsed.key));
  return parts.join(mac ? "" : "+");
}

function displayKey(key: string): string {
  if (key === "space") return "Space";
  if (key.length === 1) return key.toUpperCase();
  // "arrowdown" -> "ArrowDown", "delete" -> "Delete".
  return key.charAt(0).toUpperCase() + key.slice(1);
}
