/**
 * Print a single open message (Track 4.6).
 *
 * The document is built from the SAME sanitized payload the reading pane shows
 * (ADR-0007): the body is only ever the backend's already-sanitized HTML
 * (script- and active-content-free) or the plain-text body escaped into a
 * `<pre>`; every header field is HTML-escaped. No unsanitized markup can reach
 * the print document, so a script embedded in the original message never runs
 * when it is printed.
 *
 * Printing renders into a hidden, script-less iframe and calls that iframe's
 * own `print()`, so only the message — no app chrome — is printed, and long
 * messages paginate naturally. The sandbox withholds `allow-scripts` (the
 * sanitized content has no scripts, and none could execute even if present);
 * `allow-same-origin` lets the parent trigger the print and `allow-modals` lets
 * the print dialog open.
 */
import type { MessageDetailDto, RecipientDto } from "../types/mail";

/** HTML-escapes text destined for element content or a double-quoted attribute. */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Renders a recipient list as `Name <addr>, addr, …`. */
function recipientLine(list: RecipientDto[]): string {
  return list
    .map((entry) => (entry.name ? `${entry.name} <${entry.address}>` : entry.address))
    .join(", ");
}

/** Formats a Unix-seconds timestamp for display, or `""` when absent. */
function formatDate(unix: number | null): string {
  if (unix === null) return "";
  return new Date(unix * 1000).toLocaleString();
}

/** A header table row, or `""` when the value is empty. */
function headerRow(label: string, value: string): string {
  if (!value) return "";
  return `<tr><th scope="row">${escapeHtml(label)}</th><td>${escapeHtml(value)}</td></tr>`;
}

/** Minimal print styling; degrades gracefully if a strict CSP strips it. */
const PRINT_CSS = `
  @page { margin: 1.6cm; }
  body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; color: #111; margin: 0; }
  .print-subject { font-size: 1.4rem; margin: 0 0 0.6rem; }
  .print-headers { border-collapse: collapse; font-size: 0.85rem; margin-bottom: 0.4rem; }
  .print-headers th { text-align: left; padding-right: 0.8rem; color: #555; vertical-align: top; font-weight: 600; }
  .print-headers td { vertical-align: top; }
  hr { border: none; border-top: 1px solid #ccc; margin: 0.6rem 0 1rem; }
  .print-body { font-size: 0.95rem; line-height: 1.5; }
  .text-body { white-space: pre-wrap; overflow-wrap: anywhere; font-family: inherit; }
`;

/**
 * Builds the self-contained print document for `detail`. Pure and
 * deterministic — the security-critical unit under test.
 */
export function buildPrintDocument(detail: MessageDetailDto): string {
  const subject = detail.subject ?? "(no subject)";
  const from = detail.fromName
    ? `${detail.fromName} <${detail.fromAddress}>`
    : detail.fromAddress;
  const rows = [
    headerRow("From", from),
    headerRow("To", recipientLine(detail.to)),
    headerRow("Cc", recipientLine(detail.cc)),
    headerRow("Date", formatDate(detail.dateUnix)),
  ].join("");
  // The body is the backend-sanitized HTML verbatim (already script-free), or
  // the plain-text body escaped into a <pre> — never raw markup.
  const body =
    detail.sanitizedHtml !== null
      ? detail.sanitizedHtml
      : `<pre class="text-body">${escapeHtml(detail.textBody ?? "")}</pre>`;
  return [
    "<!doctype html>",
    '<html lang="en"><head><meta charset="utf-8">',
    `<title>${escapeHtml(subject)}</title>`,
    `<style>${PRINT_CSS}</style>`,
    "</head><body>",
    `<h1 class="print-subject">${escapeHtml(subject)}</h1>`,
    `<table class="print-headers"><tbody>${rows}</tbody></table>`,
    "<hr>",
    `<div class="print-body">${body}</div>`,
    "</body></html>",
  ].join("");
}

/**
 * Renders `detail` into a hidden, script-less iframe and prints it. A no-op
 * when there is no DOM (SSR/tests without a document). The iframe is removed
 * shortly after the print dialog is dismissed.
 */
export function printMessageDocument(detail: MessageDetailDto): void {
  if (typeof document === "undefined") return;
  const html = buildPrintDocument(detail);
  const iframe = document.createElement("iframe");
  iframe.setAttribute("aria-hidden", "true");
  iframe.setAttribute("sandbox", "allow-same-origin allow-modals");
  iframe.style.cssText =
    "position:fixed;left:-9999px;top:0;width:1px;height:1px;border:0;";
  iframe.addEventListener("load", () => {
    const frameWindow = iframe.contentWindow;
    if (frameWindow === null || typeof frameWindow.print !== "function") {
      iframe.remove();
      return;
    }
    frameWindow.focus();
    frameWindow.print();
    // Leave the frame briefly so the print dialog can capture it, then remove.
    window.setTimeout(() => iframe.remove(), 1000);
  });
  document.body.appendChild(iframe);
  iframe.srcdoc = html;
}
