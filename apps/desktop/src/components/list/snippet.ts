/**
 * Parsing for FTS5 search snippets.
 *
 * The backend builds snippets with `snippet(messages_fts, …, '[', ']', …)`,
 * so matched terms arrive wrapped in `[`/`]` markers inside otherwise plain
 * text. This splits a snippet into ordered segments so the view can style
 * matches (or drop the markers) using text interpolation — the markup is
 * NEVER derived from the snippet, so no HTML can be injected from mail
 * content.
 */

/** One run of snippet text, flagged as a matched term or surrounding text. */
export interface SnippetSegment {
  text: string;
  match: boolean;
}

const MARKER = /\[([^\]]*)\]/g;

/**
 * Splits a snippet into plain and matched segments. Text outside `[`/`]`
 * pairs is returned as `match: false`; text inside as `match: true`. Input
 * without markers yields a single non-matching segment.
 */
export function parseSnippet(snippet: string): SnippetSegment[] {
  const segments: SnippetSegment[] = [];
  let lastIndex = 0;
  MARKER.lastIndex = 0;
  let hit: RegExpExecArray | null;
  while ((hit = MARKER.exec(snippet)) !== null) {
    if (hit.index > lastIndex) {
      segments.push({ text: snippet.slice(lastIndex, hit.index), match: false });
    }
    segments.push({ text: hit[1] ?? "", match: true });
    lastIndex = MARKER.lastIndex;
  }
  if (lastIndex < snippet.length) {
    segments.push({ text: snippet.slice(lastIndex), match: false });
  }
  return segments;
}
