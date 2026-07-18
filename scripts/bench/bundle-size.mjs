#!/usr/bin/env node
/**
 * Reports release binary size and frontend production-bundle sizes
 * (raw + gzip, computed locally with node:zlib — no network fetch of a
 * "real" gzip tool involved).
 *
 * Run: node scripts/bench/bundle-size.mjs
 * Or:  pnpm bench:size
 *
 * Prerequisites: `cargo build --release -p axomail-desktop` and
 * `pnpm build` (or `pnpm --filter axomail-desktop build`) must have run
 * at least once; this script only reads existing output, it does not
 * build anything itself.
 */
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { failSetup, hostContext, printResult } from "./lib/report.mjs";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const BINARY_PATH = path.join(REPO_ROOT, "target", "release", "axomail-desktop.exe");
const DIST_DIR = path.join(REPO_ROOT, "apps", "desktop", "dist");
const FRONTEND_EXTENSIONS = new Set([".js", ".css"]);
const GZIP_OPTIONS = { level: 9 };

/** Recursively lists every file under `dir` matching `FRONTEND_EXTENSIONS`. */
function* walkFrontendFiles(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walkFrontendFiles(full);
    } else if (FRONTEND_EXTENSIONS.has(path.extname(entry.name))) {
      yield full;
    }
  }
}

function measureFrontendFile(filePath) {
  const raw = readFileSync(filePath);
  const gzip = gzipSync(raw, GZIP_OPTIONS);
  return {
    file: path.relative(DIST_DIR, filePath).split(path.sep).join("/"),
    rawBytes: raw.byteLength,
    gzipBytes: gzip.byteLength,
  };
}

function main() {
  if (!existsSync(BINARY_PATH)) {
    failSetup("release binary not found; run: cargo build --release -p axomail-desktop", {
      binary: BINARY_PATH,
    });
    return;
  }
  if (!existsSync(DIST_DIR)) {
    failSetup("frontend dist not found; run: pnpm build", { dist: DIST_DIR });
    return;
  }

  const binaryBytes = statSync(BINARY_PATH).size;

  let frontendFiles;
  try {
    frontendFiles = [...walkFrontendFiles(DIST_DIR)].map(measureFrontendFile);
  } catch (err) {
    failSetup(`failed to read frontend dist: ${err.message}`, { dist: DIST_DIR });
    return;
  }
  if (frontendFiles.length === 0) {
    failSetup("no .js/.css files found under frontend dist; is the build stale or empty?", {
      dist: DIST_DIR,
    });
    return;
  }

  const totals = frontendFiles.reduce(
    (acc, f) => ({
      rawBytes: acc.rawBytes + f.rawBytes,
      gzipBytes: acc.gzipBytes + f.gzipBytes,
    }),
    { rawBytes: 0, gzipBytes: 0 },
  );

  printResult({
    ok: true,
    script: "bundle-size.mjs",
    ...hostContext(),
    binary: { path: BINARY_PATH, bytes: binaryBytes },
    frontend: {
      dist: DIST_DIR,
      files: frontendFiles,
      totalRawBytes: totals.rawBytes,
      totalGzipBytes: totals.gzipBytes,
    },
  });
}

main();
