/**
 * Pure helpers for the rich-text composer body: an allowlist sanitizer, the
 * HTML <-> plain-text degradations, an HTML signature renderer, and the
 * attachment-keyword heuristic.
 *
 * These are the client-side mirror of the reader's `mail-security` allowlist
 * (ADR-0007): the contenteditable surface produces HTML with `document.exec-
 * Command`, and every value that leaves the editor is reduced here to a known,
 * script-free subset. The Rust send path re-sanitizes with the authoritative
 * `sanitize_message_html` — this is the first of the two layers, not the only
 * one. All functions are deterministic and never execute the input: parsing
 * goes through `DOMParser`, whose document is inert (no script runs, no remote
 * fetch), so hostile markup is defused before it is ever inspected.
 */

/** Inline/block elements allowed to survive in a composed body. */
const ALLOWED_TAGS = new Set([
  "p",
  "div",
  "br",
  "b",
  "strong",
  "i",
  "em",
  "u",
  "s",
  "strike",
  "sub",
  "sup",
  "ul",
  "ol",
  "li",
  "blockquote",
  "pre",
  "code",
  "span",
  "a",
]);

/**
 * Elements whose entire subtree is dropped rather than unwrapped: keeping their
 * text would leak scripts, styles, or form defaults into the message body.
 */
const DROP_SUBTREE_TAGS = new Set([
  "script",
  "style",
  "iframe",
  "object",
  "embed",
  "applet",
  "svg",
  "math",
  "form",
  "input",
  "textarea",
  "select",
  "button",
  "link",
  "meta",
  "base",
  "title",
  "head",
  "noscript",
  "template",
  "frame",
  "frameset",
  "img",
]);

/** URL schemes permitted on a link's `href`. */
const LINK_SCHEMES = new Set(["http", "https", "mailto"]);

/** Escapes the five HTML-significant characters in `text`. */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/**
 * Reduces arbitrary editor HTML to the allowlisted subset. Disallowed
 * structural elements are unwrapped (their safe children are kept); dangerous
 * elements are removed whole; every attribute is stripped except a link's
 * `href` when it carries a web or mail scheme. Comments are removed.
 */
export function sanitizeEditorHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  sanitizeChildren(doc.body);
  return doc.body.innerHTML;
}

/** Recursively sanitizes the children of `parent`, mutating in place. */
function sanitizeChildren(parent: Node): void {
  for (const node of Array.from(parent.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) continue;
    if (node.nodeType !== Node.ELEMENT_NODE) {
      parent.removeChild(node);
      continue;
    }
    const element = node as Element;
    const tag = element.tagName.toLowerCase();
    if (DROP_SUBTREE_TAGS.has(tag)) {
      parent.removeChild(element);
      continue;
    }
    sanitizeChildren(element);
    if (ALLOWED_TAGS.has(tag)) {
      cleanAttributes(element, tag);
    } else {
      unwrap(element, parent);
    }
  }
}

/** Strips every attribute except an allowed-scheme `href` on a link. */
function cleanAttributes(element: Element, tag: string): void {
  for (const attr of Array.from(element.attributes)) {
    const keep =
      tag === "a" &&
      attr.name.toLowerCase() === "href" &&
      hasAllowedScheme(attr.value);
    if (!keep) element.removeAttribute(attr.name);
  }
}

/** Moves an element's children up into `parent`, then removes the element. */
function unwrap(element: Element, parent: Node): void {
  while (element.firstChild !== null) {
    parent.insertBefore(element.firstChild, element);
  }
  parent.removeChild(element);
}

/** True when `value` begins with an allowed (`http`/`https`/`mailto`) scheme. */
function hasAllowedScheme(value: string): boolean {
  const scheme = urlScheme(value);
  return scheme !== null && LINK_SCHEMES.has(scheme);
}

/**
 * Extracts the lowercase URL scheme of `value`, ignoring leading whitespace.
 * Returns `null` for a relative or scheme-less reference. The scheme must start
 * with a letter, so a control-prefixed pseudo-scheme (e.g. a `javascript:` URL
 * hidden behind a leading control byte) fails closed and its link is dropped —
 * matching the reader sanitizer's fail-closed behavior.
 */
export function urlScheme(value: string): string | null {
  const trimmed = value.replace(/^\s+/, "");
  const colon = trimmed.indexOf(":");
  if (colon <= 0) return null;
  const scheme = trimmed.slice(0, colon);
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*$/.test(scheme)) return null;
  return scheme.toLowerCase();
}

/** Renders plain text as a minimal HTML fragment (escaped, newlines to `<br>`). */
export function textToHtml(text: string): string {
  if (text.length === 0) return "";
  return escapeHtml(text).replace(/\r?\n/g, "<br>");
}

/**
 * Degrades HTML to a plain-text alternative: block elements and `<br>` become
 * newlines, list items gain markers, blockquotes are prefixed with `> `, and a
 * link whose text differs from its target keeps the target in angle brackets.
 * Runs of blank lines are collapsed.
 */
export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const raw = renderText(doc.body, "");
  return raw
    .replace(/[^\S\r\n]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** Recursively renders a node to text, `quote` prefixing blockquote lines. */
function renderText(node: Node, quote: string): string {
  let out = "";
  let index = 0;
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      out += (child.textContent ?? "").replace(/\s+/g, " ");
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const element = child as Element;
    const tag = element.tagName.toLowerCase();
    index += 1;
    out += renderElement(element, tag, index, quote);
  }
  return out;
}

/** Renders one element to text based on its tag. */
function renderElement(
  element: Element,
  tag: string,
  index: number,
  quote: string,
): string {
  if (tag === "br") return "\n";
  if (tag === "a") return renderLink(element, quote);
  if (tag === "li") {
    const ordered = element.parentElement?.tagName.toLowerCase() === "ol";
    const marker = ordered ? `${index}. ` : "- ";
    return `${marker}${renderText(element, quote).trim()}\n`;
  }
  if (tag === "blockquote") {
    const inner = renderText(element, `${quote}> `).trim();
    const quoted = inner
      .split("\n")
      .map((line) => `${quote}> ${line}`.trimEnd())
      .join("\n");
    return `${quoted}\n`;
  }
  if (BLOCK_TAGS.has(tag)) return `${renderText(element, quote).trim()}\n`;
  return renderText(element, quote);
}

/** Block tags that force a line break around their content. */
const BLOCK_TAGS = new Set([
  "p",
  "div",
  "ul",
  "ol",
  "pre",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
]);

/** Renders a link as its text, appending ` <href>` when they differ. */
function renderLink(element: Element, quote: string): string {
  const text = renderText(element, quote).trim();
  const href = element.getAttribute("href") ?? "";
  if (href.length === 0 || href === text) return text;
  return text.length === 0 ? href : `${text} <${href}>`;
}

/** The plain-text signature delimiter line (dash, dash, space; RFC 3676). */
export const SIGNATURE_DELIMITER = "-- ";

/**
 * Renders a plain-text signature as the trailing HTML block appended to an
 * HTML body: two line breaks, the delimiter, then each signature line. Returns
 * an empty string for blank input so a signature-less identity adds nothing.
 */
export function signatureHtml(text: string | null | undefined): string {
  if (text === null || text === undefined || text.trim().length === 0) {
    return "";
  }
  const body = escapeHtml(text).replace(/\r?\n/g, "<br>");
  return `<br><br>${escapeHtml(SIGNATURE_DELIMITER)}<br>${body}`;
}

/**
 * Localized keywords that suggest the writer meant to attach a file. Matched as
 * lowercased substrings so inflections (attached, attaching) are covered.
 */
const ATTACHMENT_KEYWORDS = [
  "attach",
  "enclosed",
  "anbei",
  "anhang",
  "angehängt",
  "angehaengt",
  "beigefügt",
  "beigefuegt",
  "beifügung",
];

/** True when `text` mentions attaching something (any supported language). */
export function mentionsAttachmentKeyword(text: string): boolean {
  const lower = text.toLowerCase();
  return ATTACHMENT_KEYWORDS.some((keyword) => lower.includes(keyword));
}
