import { describe, expect, it } from "vitest";

import { applySignature, signatureBlock } from "../src/lib/signature";

describe("signatureBlock", () => {
  it("prefixes the standard delimiter and blank lines", () => {
    expect(signatureBlock("Klaus")).toBe("\n\n-- \nKlaus");
  });

  it("returns nothing for blank or absent text", () => {
    expect(signatureBlock("")).toBe("");
    expect(signatureBlock("   ")).toBe("");
    expect(signatureBlock(null)).toBe("");
    expect(signatureBlock(undefined)).toBe("");
  });
});

describe("applySignature", () => {
  it("appends the new block to a body without one", () => {
    const block = signatureBlock("Klaus");
    expect(applySignature("Hello", "", block)).toBe(`Hello${block}`);
  });

  it("swaps an existing block in place", () => {
    const oldBlock = signatureBlock("Klaus");
    const newBlock = signatureBlock("Sales");
    const body = `Hello${oldBlock}`;
    expect(applySignature(body, oldBlock, newBlock)).toBe(`Hello${newBlock}`);
  });

  it("never removes user text edited below the old block", () => {
    const oldBlock = signatureBlock("Klaus");
    const newBlock = signatureBlock("Sales");
    // The user typed after the signature, so it no longer ends the body.
    const body = `Hello${oldBlock}\nP.S. more`;
    expect(applySignature(body, oldBlock, newBlock)).toBe(`${body}${newBlock}`);
  });

  it("replaces an existing trailing signature block when none was tracked", () => {
    // A draft loaded with a signature already baked into its body: no block was
    // inserted this session (oldBlock is empty), so the delimiter must be found
    // and the old signature replaced — not a second one appended.
    const newBlock = signatureBlock("Sales");
    const body = "Hello there\n\n-- \nKlaus";
    const result = applySignature(body, "", newBlock);

    expect(result).toBe(`Hello there${newBlock}`);
    // Exactly one signature delimiter remains.
    expect(result.match(/-- \n/g)).toHaveLength(1);
    expect(result).not.toContain("Klaus");
  });

  it("replaces a multi-line trailing signature, keeping the body above it", () => {
    const oldBlock = signatureBlock("Cheers,\nKlaus");
    const newBlock = signatureBlock("Best,\nSales");
    const body = `Draft body\n\nsecond paragraph${oldBlock}`;
    expect(applySignature(body, "", newBlock)).toBe(
      `Draft body\n\nsecond paragraph${newBlock}`,
    );
  });

  it("leaves a body with no signature untouched when none is tracked", () => {
    const newBlock = signatureBlock("Sales");
    expect(applySignature("Just plain text", "", newBlock)).toBe(
      `Just plain text${newBlock}`,
    );
  });
});
