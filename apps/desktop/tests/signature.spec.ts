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
});
