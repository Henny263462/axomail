/**
 * Plain-text signature helpers for the composer.
 *
 * Signatures are inserted with the standard delimiter line `"-- "` (dash,
 * dash, space) followed by a newline, per RFC 3676 section 4.3. Keeping the
 * logic here (a) makes it unit-testable and (b) lets the composer swap the
 * signature in place when the From identity changes.
 */

/** The standard signature delimiter line: dash, dash, space. */
export const SIGNATURE_DELIMITER = "-- ";

/**
 * Builds the signature block appended to a message body: two blank lines, the
 * delimiter, then the signature text. Returns an empty string for blank input
 * so a signature-less identity contributes nothing.
 */
export function signatureBlock(text: string | null | undefined): string {
  if (text === null || text === undefined || text.trim().length === 0) {
    return "";
  }
  return `\n\n${SIGNATURE_DELIMITER}\n${text}`;
}

/**
 * Removes a trailing signature block from `body`: everything from the last
 * standalone delimiter line ("-- ") to the end, plus the blank separator lines
 * that precede it. Text above the delimiter is preserved untouched. Returns
 * `body` unchanged when it carries no delimiter line.
 *
 * Used when no in-session block is tracked (e.g. a draft loaded with a
 * signature already baked into its body) so an identity switch replaces the
 * existing signature instead of appending a second one.
 */
function stripTrailingSignature(body: string): string {
  const lines = body.split("\n");
  let delimiter = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    if (lines[i] === SIGNATURE_DELIMITER) {
      delimiter = i;
      break;
    }
  }
  if (delimiter === -1) return body;
  const above = lines.slice(0, delimiter);
  while (above.length > 0 && above[above.length - 1] === "") above.pop();
  return above.join("\n");
}

/**
 * Swaps the signature block at the end of `body`, then appends `newBlock`.
 *
 * When `oldBlock` is a tracked in-session block, it is stripped only if the
 * body still ends with it — if the user typed below it, the new block is
 * appended without touching their text (nothing is lost). When no block is
 * tracked (`oldBlock` is empty), an existing trailing signature block is
 * detected by its delimiter and replaced, so switching identities on a loaded
 * draft never duplicates the signature. Body text above the delimiter is
 * always preserved.
 */
export function applySignature(
  body: string,
  oldBlock: string,
  newBlock: string,
): string {
  if (oldBlock.length > 0) {
    const base = body.endsWith(oldBlock)
      ? body.slice(0, body.length - oldBlock.length)
      : body;
    return base + newBlock;
  }
  return stripTrailingSignature(body) + newBlock;
}
