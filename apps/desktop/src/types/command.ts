/**
 * Shared command-registry contract.
 *
 * A **command** is a named, runnable action with an optional keyboard shortcut.
 * The registry ({@link "../composables/useCommandRegistry"}) is the dependency-
 * free foundation that Wave 4 builds on: track 4.5 renders the command palette
 * and help overlay from the registered command list, and the action tracks
 * (4.1 contacts, 4.2 filters, 4.3 junk, plus reply/move/etc.) register their
 * user-facing actions as commands so they appear in the palette and gain
 * shortcuts without each inventing its own keydown plumbing.
 *
 * This module defines only the data types; the composable owns the registry,
 * the shortcut parser, and the global keydown dispatcher.
 */

/**
 * A keyboard-shortcut spec: modifier tokens and a final key, joined by `+`.
 *
 * - Modifiers (case-insensitive): `Ctrl`/`Control`, `Alt`/`Option`, `Shift`,
 *   `Meta`/`Cmd`/`Command`/`Super`/`Win`, and `Mod` — the platform's primary
 *   accelerator, which resolves to `Meta` on macOS and `Ctrl` elsewhere. Prefer
 *   `Mod` for app accelerators (e.g. the palette's `Mod+K`) so one spec is
 *   correct on every OS.
 * - The key is matched against `KeyboardEvent.key` case-insensitively, so use
 *   its value: `"k"`, `"Delete"`, `"Escape"`, `"ArrowDown"`, `"Space"`.
 *
 * Examples: `"Mod+K"`, `"Ctrl+Shift+A"`, `"n"`, `"Delete"`.
 */
export type Shortcut = string;

/**
 * A registered command.
 *
 * `run` may be async; the dispatcher fires it without awaiting. Keep `id`
 * namespaced (`"<feature>.<action>"`, e.g. `"message.reply"`) so features never
 * collide; re-registering the same `id` replaces the prior command.
 */
export interface Command {
  /** Stable, namespaced unique id (e.g. `"contacts.new"`). */
  id: string;
  /** Human-readable label shown in the palette / help overlay. */
  title: string;
  /**
   * Extra search terms the palette matches in addition to {@link title}
   * (synonyms, the feature name). Empty when the title alone suffices.
   */
  keywords: readonly string[];
  /** Optional shortcut whose key chord runs the command globally. */
  shortcut?: Shortcut;
  /** Runs the command. May be async; the dispatcher does not await it. */
  run: () => void | Promise<void>;
  /**
   * Optional availability gate. When it returns `false`, the shortcut does not
   * fire (the keystroke falls through) and the palette should render the entry
   * as disabled. Absent means always available.
   */
  isEnabled?: () => boolean;
}
