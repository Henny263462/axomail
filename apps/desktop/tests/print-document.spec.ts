import { beforeEach, describe, expect, it } from "vitest";

import { buildPrintDocument, printMessageDocument } from "../src/lib/printDocument";
import type { MessageDetailDto } from "../src/types/mail";

function detail(over: Partial<MessageDetailDto> = {}): MessageDetailDto {
  return {
    id: 1,
    subject: "Subject",
    fromName: "Ada",
    fromAddress: "ada@example.org",
    to: [],
    cc: [],
    dateUnix: 1_752_000_000,
    flagged: false,
    textBody: "body",
    sanitizedHtml: null,
    remoteBlockedCount: 0,
    remoteAllowed: false,
    scamFlags: [],
    hadActiveContent: false,
    attachments: [],
    messageId: null,
    inReplyTo: [],
    references: [],
    ...over,
  };
}

describe("buildPrintDocument", () => {
  it("includes the subject, sender, recipients, and the sanitized HTML body", () => {
    const doc = buildPrintDocument(
      detail({
        subject: "Quarterly report",
        fromName: "Ada Lovelace",
        fromAddress: "ada@example.org",
        to: [{ name: "Bob", address: "bob@example.com" }],
        cc: [{ name: null, address: "carol@example.com" }],
        sanitizedHtml: "<p>Hello <b>world</b></p>",
        textBody: "Hello world",
      }),
    );
    expect(doc.startsWith("<!doctype html>")).toBe(true);
    expect(doc).toContain("<title>Quarterly report</title>");
    expect(doc).toContain("Quarterly report");
    expect(doc).toContain("ada@example.org");
    expect(doc).toContain("bob@example.com");
    expect(doc).toContain("carol@example.com");
    // The sanitized HTML is embedded verbatim (it is the trusted body source).
    expect(doc).toContain("<p>Hello <b>world</b></p>");
  });

  it("escapes header fields so markup in them cannot inject nodes", () => {
    const doc = buildPrintDocument(
      detail({
        subject: "<script>alert(1)</script>",
        fromName: "<img src=x onerror=alert(1)>",
        fromAddress: "evil@example.org",
        sanitizedHtml: null,
        textBody: "safe",
      }),
    );
    expect(doc).not.toContain("<script>alert(1)</script>");
    expect(doc).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(doc).not.toContain("<img src=x onerror=alert(1)>");
    expect(doc).toContain("&lt;img src=x onerror=alert(1)&gt;");
  });

  it("escapes a plain-text body into a <pre>, never emitting it as markup", () => {
    const doc = buildPrintDocument(
      detail({ sanitizedHtml: null, textBody: "<script>alert('x')</script>\nline two" }),
    );
    expect(doc).toContain('<pre class="text-body">');
    expect(doc).not.toContain("<script>alert('x')</script>");
    // Single quotes are not escaped (they are inert in this context); the angle
    // brackets are, which is what neutralizes the tag.
    expect(doc).toContain("&lt;script&gt;alert('x')&lt;/script&gt;");
  });

  it("never emits an adversarial text body when sanitized HTML is present", () => {
    // The DTO's sanitizedHtml is already script-free (Rust ammonia, ADR-0007).
    // The builder must have exactly one body path and never fall back to the raw
    // textBody, so a script that only lives in textBody cannot reach the print
    // document.
    const doc = buildPrintDocument(
      detail({
        sanitizedHtml: "<p>clean</p>",
        textBody: "<script>alert('body')</script>",
      }),
    );
    expect(doc).toContain("<p>clean</p>");
    expect(doc).not.toContain("<script>alert('body')</script>");
  });
});

describe("printMessageDocument", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  it("renders into a script-less sandboxed iframe (ADR-0007)", () => {
    printMessageDocument(detail({ subject: "Print me", sanitizedHtml: "<p>hi</p>" }));
    const iframe = document.querySelector("iframe");
    expect(iframe).not.toBeNull();
    const sandbox = iframe?.getAttribute("sandbox") ?? "";
    // No script execution in the print frame; a modal is allowed for the dialog.
    expect(sandbox).not.toContain("allow-scripts");
    expect(sandbox).toContain("allow-modals");
    expect(iframe?.getAttribute("aria-hidden")).toBe("true");
  });

  it("puts the sanitized document into the frame, not a raw-markup body", () => {
    printMessageDocument(detail({ sanitizedHtml: null, textBody: "<script>x()</script>" }));
    const iframe = document.querySelector("iframe") as HTMLIFrameElement | null;
    const srcdoc = iframe?.srcdoc ?? iframe?.getAttribute("srcdoc") ?? "";
    expect(srcdoc).not.toContain("<script>x()</script>");
    expect(srcdoc).toContain("&lt;script&gt;x()&lt;/script&gt;");
  });
});
