import { describe, expect, it } from "vitest";

import {
  htmlToText,
  mentionsAttachmentKeyword,
  sanitizeEditorHtml,
  signatureHtml,
  textToHtml,
  urlScheme,
} from "../src/lib/htmlBody";

describe("sanitizeEditorHtml", () => {
  it("keeps the allowlisted formatting subset", () => {
    const input =
      "<p>Hello <b>bold</b> <i>italic</i> <u>under</u></p>" +
      "<ul><li>one</li><li>two</li></ul>" +
      "<blockquote>quote</blockquote>";
    const out = sanitizeEditorHtml(input);
    expect(out).toContain("<b>bold</b>");
    expect(out).toContain("<i>italic</i>");
    expect(out).toContain("<u>under</u>");
    expect(out).toContain("<li>one</li>");
    expect(out).toContain("<blockquote>quote</blockquote>");
  });

  it("removes script, style, and event-handler attributes", () => {
    const hostile =
      '<p onclick="steal()">hi</p><script>alert(1)</script>' +
      '<style>body{display:none}</style><b onmouseover="x()">b</b>';
    const out = sanitizeEditorHtml(hostile);
    expect(out).not.toContain("script");
    expect(out).not.toContain("alert(1)");
    expect(out).not.toContain("style");
    expect(out).not.toContain("onclick");
    expect(out).not.toContain("onmouseover");
    // The visible text and its bold formatting survive.
    expect(out).toContain("hi");
    expect(out).toContain("<b>b</b>");
  });

  it("drops an img with an onerror handler entirely", () => {
    const out = sanitizeEditorHtml('<p>x<img src="x" onerror="alert(1)">y</p>');
    expect(out).not.toContain("img");
    expect(out).not.toContain("onerror");
    expect(out).toContain("x");
    expect(out).toContain("y");
  });

  it("keeps web and mail links but drops javascript: and data: hrefs", () => {
    const out = sanitizeEditorHtml(
      '<a href="https://example.com">ok</a>' +
        '<a href="mailto:a@b.com">mail</a>' +
        '<a href="javascript:alert(1)">bad</a>' +
        '<a href="data:text/html,x">data</a>',
    );
    expect(out).toContain('href="https://example.com"');
    expect(out).toContain('href="mailto:a@b.com"');
    expect(out).not.toContain("javascript:");
    expect(out).not.toContain("data:");
    // The disallowed-scheme anchors keep their text, without an href.
    expect(out).toContain("bad");
    expect(out).toContain("data");
  });

  it("strips style, class, and id attributes while keeping the element", () => {
    const out = sanitizeEditorHtml(
      '<p class="c" id="i" style="color:red">text</p>',
    );
    expect(out).toBe("<p>text</p>");
  });

  it("unwraps unknown tags, preserving their text", () => {
    const out = sanitizeEditorHtml("<marquee>run <b>fast</b></marquee>");
    expect(out).not.toContain("marquee");
    expect(out).toContain("run <b>fast</b>");
  });

  it("survives a control-byte obfuscated javascript URL (fails closed)", () => {
    const out = sanitizeEditorHtml('<a href="javascript:alert(1)">x</a>');
    expect(out).not.toContain("javascript");
    expect(out).toContain("x");
  });
});

describe("urlScheme", () => {
  it("parses and lowercases known schemes", () => {
    expect(urlScheme("HTTPS://x")).toBe("https");
    expect(urlScheme("  mailto:a@b")).toBe("mailto");
    expect(urlScheme("/relative")).toBeNull();
    expect(urlScheme("noscheme")).toBeNull();
  });
});

describe("htmlToText", () => {
  it("flattens formatting to plain text", () => {
    expect(htmlToText("<p>Hello <b>bold</b> world</p>")).toBe("Hello bold world");
  });

  it("turns <br> into newlines", () => {
    expect(htmlToText("line one<br>line two")).toBe("line one\nline two");
  });

  it("marks unordered and ordered list items", () => {
    expect(htmlToText("<ul><li>a</li><li>b</li></ul>")).toBe("- a\n- b");
    expect(htmlToText("<ol><li>a</li><li>b</li></ol>")).toBe("1. a\n2. b");
  });

  it("prefixes blockquote lines with a marker", () => {
    expect(htmlToText("<blockquote>quoted</blockquote>")).toBe("> quoted");
  });

  it("keeps a link's target when it differs from its text", () => {
    expect(htmlToText('<a href="https://x.test">site</a>')).toBe(
      "site <https://x.test>",
    );
  });
});

describe("textToHtml", () => {
  it("escapes and converts newlines to <br>", () => {
    expect(textToHtml("a < b\nc & d")).toBe("a &lt; b<br>c &amp; d");
  });
});

describe("signatureHtml", () => {
  it("renders a delimiter block, escaping the text", () => {
    const out = signatureHtml("Klaus <k@x>");
    expect(out).toContain("-- ");
    expect(out).toContain("Klaus &lt;k@x&gt;");
    expect(out.startsWith("<br><br>")).toBe(true);
  });

  it("returns empty for a blank signature", () => {
    expect(signatureHtml("")).toBe("");
    expect(signatureHtml(null)).toBe("");
  });
});

describe("mentionsAttachmentKeyword", () => {
  it("detects English and German attachment phrasing", () => {
    expect(mentionsAttachmentKeyword("Please see the attached file")).toBe(true);
    expect(mentionsAttachmentKeyword("Die Datei ist anbei")).toBe(true);
    expect(mentionsAttachmentKeyword("Siehe Anhang")).toBe(true);
    expect(mentionsAttachmentKeyword("angehängt findest du")).toBe(true);
  });

  it("does not fire on unrelated text", () => {
    expect(mentionsAttachmentKeyword("Let us meet at noon")).toBe(false);
  });
});
