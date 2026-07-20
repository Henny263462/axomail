/**
 * Dependency-free fuzzy filtering for the command palette.
 *
 * A query matches a target when its characters appear in order (a subsequence),
 * case-insensitively. The score rewards contiguous runs and word-boundary hits
 * so `"nm"` ranks `"New message"` above an incidental scatter match, and a
 * shorter target edges out a longer one on a tie. The matched character ranges
 * are returned so the palette can highlight them.
 */
import type { Command } from "../../types/command";

/** A successful match: its relevance score and the matched title ranges. */
export interface FuzzyResult {
  /** Higher is a better match. */
  score: number;
  /** Half-open `[start, end)` slices of the target that matched. */
  ranges: [number, number][];
}

/**
 * Subsequence-matches `query` against `text`. Returns `null` when `text` does
 * not contain the query characters in order; an empty query matches everything
 * with no highlight.
 */
export function fuzzyMatch(query: string, text: string): FuzzyResult | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q === "") return { score: 0, ranges: [] };

  const ranges: [number, number][] = [];
  let cursor = 0;
  let score = 0;
  let prev = -2;
  let runStart = -1;

  for (const ch of q) {
    let found = -1;
    for (let i = cursor; i < t.length; i++) {
      if (t[i] === ch) {
        found = i;
        break;
      }
    }
    if (found === -1) return null;

    const contiguous = found === prev + 1;
    score += contiguous ? 3 : 1;
    if (found === 0 || isBoundary(t[found - 1])) score += 2;

    if (runStart === -1) {
      runStart = found;
    } else if (!contiguous) {
      ranges.push([runStart, prev + 1]);
      runStart = found;
    }
    prev = found;
    cursor = found + 1;
  }
  ranges.push([runStart, prev + 1]);
  // Nudge shorter targets ahead of longer ones on an otherwise equal score.
  score -= t.length * 0.01;
  return { score, ranges };
}

/** A command that survived the filter, with its title highlight ranges. */
export interface RankedCommand {
  command: Command;
  /** Title ranges to highlight; empty when only a keyword matched. */
  ranges: [number, number][];
}

/**
 * Filters and ranks `commands` for `query`. An empty query returns every
 * command in registration order (no highlight). Otherwise a command is kept
 * when its title or any keyword matches; ranking is by best score, ties broken
 * by original order. Highlight ranges are the title's (a keyword-only match
 * highlights nothing).
 */
export function filterCommands(
  commands: readonly Command[],
  query: string,
): RankedCommand[] {
  const q = query.trim();
  if (q === "") return commands.map((command) => ({ command, ranges: [] }));

  const scored: {
    command: Command;
    ranges: [number, number][];
    score: number;
    order: number;
  }[] = [];

  commands.forEach((command, order) => {
    const titleMatch = fuzzyMatch(q, command.title);
    let best = titleMatch?.score ?? Number.NEGATIVE_INFINITY;
    for (const keyword of command.keywords) {
      const match = fuzzyMatch(q, keyword);
      if (match !== null && match.score > best) best = match.score;
    }
    if (best === Number.NEGATIVE_INFINITY) return;
    scored.push({
      command,
      ranges: titleMatch?.ranges ?? [],
      score: best,
      order,
    });
  });

  scored.sort((a, b) => b.score - a.score || a.order - b.order);
  return scored.map(({ command, ranges }) => ({ command, ranges }));
}

/** A run of title text, flagged when it is part of a fuzzy-matched range. */
export interface TitleSegment {
  text: string;
  match: boolean;
}

/** Splits `title` into matched / unmatched segments for highlight rendering. */
export function highlightSegments(
  title: string,
  ranges: readonly [number, number][],
): TitleSegment[] {
  if (ranges.length === 0) return [{ text: title, match: false }];
  const segments: TitleSegment[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (start > at) segments.push({ text: title.slice(at, start), match: false });
    segments.push({ text: title.slice(start, end), match: true });
    at = end;
  }
  if (at < title.length) segments.push({ text: title.slice(at), match: false });
  return segments;
}

function isBoundary(ch: string | undefined): boolean {
  return ch === undefined || ch === " " || ch === "-" || ch === "_" || ch === "/";
}
