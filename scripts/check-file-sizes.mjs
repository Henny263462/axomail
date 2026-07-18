#!/usr/bin/env node
/**
 * Enforces the project file-size limits (see CONTRIBUTING, section
 * "Module and file size"): guideline 300 lines, hard ceiling 400 lines,
 * counted without blank lines. Test files are exempt from the hard fail
 * but still reported.
 *
 * Covers .rs, .vue, .ts, and .css sources; ESLint's max-lines rule
 * enforces the same ceiling for frontend files during linting.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import process from "node:process";

const ROOT = process.cwd();
const GUIDELINE = 300;
const HARD_LIMIT = 400;
const EXTENSIONS = [".rs", ".vue", ".ts", ".css"];
const SKIP_DIRS = new Set(["node_modules", "target", "dist", ".git", "gen", "icons"]);

function isTestFile(path) {
  const parts = path.split(sep);
  if (parts.includes("tests")) return true;
  return /\.(spec|test)\.[cm]?ts$/.test(path) || /_test\.rs$/.test(path);
}

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      if (!SKIP_DIRS.has(entry)) yield* walk(full);
    } else if (EXTENSIONS.some((ext) => entry.endsWith(ext))) {
      yield full;
    }
  }
}

let failed = false;
for (const file of walk(ROOT)) {
  const rel = relative(ROOT, file);
  const lines = readFileSync(file, "utf8")
    .split("\n")
    .filter((line) => line.trim().length > 0).length;

  if (lines > HARD_LIMIT && !isTestFile(rel)) {
    console.error(`FAIL ${rel}: ${lines} non-blank lines (hard limit ${HARD_LIMIT})`);
    failed = true;
  } else if (lines > GUIDELINE) {
    console.warn(`warn ${rel}: ${lines} non-blank lines (guideline ${GUIDELINE})`);
  }
}

if (failed) {
  process.exit(1);
}
console.log("file-size check passed");
