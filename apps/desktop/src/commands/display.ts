/**
 * Presentation helpers shared by the command palette, the shortcut-help
 * overlay, and the Shortcuts settings section: rendering a shortcut spec for a
 * human, and bucketing commands into labelled areas.
 *
 * Grouping is derived from the command id's namespace (`"message.reply"` →
 * `message`), so no field is added to the Stage-0 {@link Command} contract and
 * feature tracks that only `register(...)` need do nothing extra — their
 * commands land under a label derived from their own namespace.
 */
import { formatShortcut } from "../composables/useCommandRegistry";
import {
  formatChord,
  isChordSequence,
  parseChordSequence,
} from "../composables/useKeyChords";
import type { Command } from "../types/command";

/**
 * Specs whose {@link formatShortcut} rendering reads awkwardly; the map gives
 * the natural label. `"Shift+?"` is how the help chord is bound (the character
 * `?` carries Shift), but users know it simply as `?`.
 */
const HINT_ALIASES: Record<string, string> = { "Shift+?": "?" };

/**
 * Renders a command shortcut for display: a chord sequence as `"G then I"`, an
 * aliased spec by its friendly label, and everything else through the registry
 * formatter (`"Ctrl+K"`, or `⌘K` glyphs on macOS). Empty for no shortcut.
 */
export function formatShortcutHint(spec: string | undefined): string {
  if (spec === undefined || spec === "") return "";
  const alias = HINT_ALIASES[spec];
  if (alias !== undefined) return alias;
  if (isChordSequence(spec)) return formatChord(parseChordSequence(spec));
  return formatShortcut(spec);
}

/** Friendly labels for the known command namespaces. */
const GROUP_LABELS: Record<string, string> = {
  app: "Application",
  compose: "Compose",
  message: "Message",
  nav: "Navigation",
  view: "View",
  contacts: "Contacts",
  filters: "Filters",
  junk: "Junk",
};

/** The namespace of a command id (`"message.reply"` → `"message"`). */
export function commandGroupId(id: string): string {
  const dot = id.indexOf(".");
  return dot === -1 ? id : id.slice(0, dot);
}

/** The display label for a command's group. */
export function commandGroupLabel(id: string): string {
  const group = commandGroupId(id);
  return GROUP_LABELS[group] ?? group.charAt(0).toUpperCase() + group.slice(1);
}

/** A labelled bucket of commands sharing a namespace. */
export interface CommandGroup {
  /** The namespace, e.g. `"message"`. */
  id: string;
  /** The display label, e.g. `"Message"`. */
  label: string;
  /** The commands in this group, in registration order. */
  commands: Command[];
}

/** Buckets commands by namespace, preserving first-seen group order. */
export function groupCommands(commands: readonly Command[]): CommandGroup[] {
  const order: string[] = [];
  const buckets = new Map<string, Command[]>();
  for (const command of commands) {
    const group = commandGroupId(command.id);
    let bucket = buckets.get(group);
    if (bucket === undefined) {
      bucket = [];
      buckets.set(group, bucket);
      order.push(group);
    }
    bucket.push(command);
  }
  return order.map((group) => ({
    id: group,
    label: commandGroupLabel(group),
    commands: buckets.get(group) ?? [],
  }));
}

/** The bindable commands (those with a shortcut), grouped for a help listing. */
export function shortcutGroups(commands: readonly Command[]): CommandGroup[] {
  return groupCommands(
    commands.filter((command) => (command.shortcut ?? "") !== ""),
  );
}
