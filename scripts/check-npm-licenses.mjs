#!/usr/bin/env node
/**
 * Verifies that all npm dependencies use MIT-compatible licenses,
 * mirroring the cargo-deny policy in deny.toml.
 *
 * Reads the output of `pnpm licenses list --json` from stdin:
 *   pnpm licenses list --json | node scripts/check-npm-licenses.mjs
 *
 * SPDX expressions are evaluated with correct precedence (AND binds
 * tighter than OR) and parentheses support. Anything unparseable is
 * treated as disallowed (fail closed).
 */
import process from "node:process";

const ALLOWED = new Set([
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "MPL-2.0",
  "Zlib",
  "BlueOak-1.0.0",
  "0BSD",
  "CC0-1.0",
  "Unlicense",
  "Python-2.0",
]);

/** Evaluates one SPDX expression; throws on malformed input. */
function evaluateSpdx(expression) {
  const tokens = expression
    .replaceAll("(", " ( ")
    .replaceAll(")", " ) ")
    .trim()
    .split(/\s+/);
  let pos = 0;

  function parseOr() {
    let allowed = parseAnd();
    while (tokens[pos] === "OR") {
      pos += 1;
      const rhs = parseAnd();
      allowed = allowed || rhs;
    }
    return allowed;
  }

  function parseAnd() {
    let allowed = parsePrimary();
    while (tokens[pos] === "AND") {
      pos += 1;
      const rhs = parsePrimary();
      allowed = allowed && rhs;
    }
    return allowed;
  }

  function parsePrimary() {
    const token = tokens[pos];
    if (token === undefined) throw new Error("unexpected end of expression");
    if (token === "(") {
      pos += 1;
      const inner = parseOr();
      if (tokens[pos] !== ")") throw new Error("unbalanced parentheses");
      pos += 1;
      return inner;
    }
    pos += 1;
    let id = token;
    if (tokens[pos] === "WITH") {
      if (tokens[pos + 1] === undefined) throw new Error("dangling WITH");
      id = `${id} WITH ${tokens[pos + 1]}`;
      pos += 2;
    }
    return ALLOWED.has(id) || ALLOWED.has(id.replace(/\+$/, ""));
  }

  const result = parseOr();
  if (pos !== tokens.length) throw new Error("trailing tokens");
  return result;
}

function isAllowed(license) {
  try {
    return evaluateSpdx(license);
  } catch {
    return false;
  }
}

let input = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) {
  input += chunk;
}

const byLicense = JSON.parse(input);
const violations = [];
for (const [license, packages] of Object.entries(byLicense)) {
  if (!isAllowed(license)) {
    for (const pkg of packages) {
      violations.push(`${pkg.name}@${pkg.versions?.join(", ") ?? "?"}: ${license}`);
    }
  }
}

if (violations.length > 0) {
  console.error("Disallowed npm licenses found:");
  for (const violation of violations) console.error(`  ${violation}`);
  process.exit(1);
}
console.log("npm license check passed");
