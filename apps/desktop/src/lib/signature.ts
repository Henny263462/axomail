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
 * Swaps a signature block at the end of `body`: strips `oldBlock` when the body
 * still ends with it (the user has not edited past it), then appends
 * `newBlock`. When the body no longer ends with the old block — because the
 * user edited below it — the new block is appended without removing anything,
 * so no user text is ever lost.
 */
export function applySignature(
  body: string,
  oldBlock: string,
  newBlock: string,
): string {
  let base = body;
  if (oldBlock.length > 0 && base.endsWith(oldBlock)) {
    base = base.slice(0, base.length - oldBlock.length);
  }
  return base + newBlock;
}
