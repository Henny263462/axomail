#!/usr/bin/env node
/**
 * Measures idle memory (Working Set, via `tasklist`) of the release
 * desktop binary once its main window exists and no user interaction is
 * happening: no account configured, nothing syncing.
 *
 * Run: node scripts/bench/idle-memory.mjs
 * Or:  pnpm bench:memory
 *
 * Method: launch the release binary, wait for its window (reusing the
 * same tasklist proxy as startup.mjs), then sample "Mem Usage" every
 * SAMPLE_INTERVAL_MS for SAMPLE_WINDOW_MS of idle wall-clock time,
 * report median/max, then kill the process tree.
 *
 * Limitations:
 * - "Mem Usage" from `tasklist` is the process Working Set in
 *   kilobytes, not private/committed bytes; it is a coarse OS-reported
 *   figure, not an in-process allocator measurement.
 * - This measures the *shell* at rest with no account configured and no
 *   mailbox data loaded — not idle memory of a populated inbox. That
 *   scenario needs its own dated measurement once phase 4 sync/storage
 *   is wired end-to-end.
 * - `pnpm tauri dev` numbers are not comparable to this script: dev
 *   mode runs an unminified frontend against a live dev server inside
 *   the WebView, plus devtools hooks, all of which inflate memory well
 *   beyond what a released build uses. Only release-binary numbers from
 *   this script belong in docs/performance/.
 */
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import process from "node:process";
import { spawn } from "node:child_process";
import { failSetup, hostContext, printResult } from "./lib/report.mjs";
import { summarize } from "./lib/stats.mjs";
import { isAlive, killTree, memUsageKb, waitForExit, windowTitle } from "./lib/win-tasklist.mjs";

const WINDOW_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 25;
const EXIT_TIMEOUT_MS = 5_000;
const SAMPLE_INTERVAL_MS = 2_000;
const SAMPLE_WINDOW_MS = 30_000;
const MIN_USABLE_SAMPLES = 5;

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const BINARY_PATH = path.join(REPO_ROOT, "target", "release", "axomail-desktop.exe");

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForWindow(pid) {
  const deadline = Date.now() + WINDOW_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (!isAlive(pid)) throw new Error("process exited before a window was observed");
    if (windowTitle(pid) !== null) return;
    // eslint-disable-next-line no-await-in-loop -- intentional poll loop
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(`no window observed within ${WINDOW_TIMEOUT_MS}ms`);
}

async function sampleIdleMemory(pid) {
  const samplesKb = [];
  const startTime = Date.now();
  let sampleIndex = 0;
  // Self-correcting cadence: schedule each sample relative to `startTime`
  // rather than chaining fixed delays, so the per-sample tasklist call
  // overhead does not compound into a short sample count over 30s.
  while (Date.now() - startTime < SAMPLE_WINDOW_MS) {
    if (!isAlive(pid)) break;
    const kb = memUsageKb(pid);
    if (kb !== null) samplesKb.push(kb);
    sampleIndex += 1;
    const nextSampleAt = startTime + sampleIndex * SAMPLE_INTERVAL_MS;
    // eslint-disable-next-line no-await-in-loop -- intentional sample loop
    await delay(Math.max(0, nextSampleAt - Date.now()));
  }
  return samplesKb;
}

async function main() {
  if (process.platform !== "win32") {
    failSetup("idle-memory.mjs only implements the tasklist sampling proxy on win32", {
      platform: process.platform,
    });
    return;
  }
  if (!existsSync(BINARY_PATH)) {
    failSetup("release binary not found; run: cargo build --release -p axomail-desktop", {
      binary: BINARY_PATH,
    });
    return;
  }

  const child = spawn(BINARY_PATH, [], { stdio: "ignore", detached: false });
  let pid;
  try {
    pid = await new Promise((resolve, reject) => {
      child.once("spawn", () => resolve(child.pid));
      child.once("error", reject);
    });
  } catch (err) {
    failSetup(`failed to spawn release binary: ${err.message}`, { binary: BINARY_PATH });
    return;
  }

  let samplesKb = [];
  let failureReason = null;
  try {
    await waitForWindow(pid);
    samplesKb = await sampleIdleMemory(pid);
  } catch (err) {
    failureReason = err.message;
  } finally {
    killTree(pid);
    await waitForExit(pid, EXIT_TIMEOUT_MS);
  }

  if (failureReason) {
    failSetup(`idle memory measurement failed: ${failureReason}`, { binary: BINARY_PATH });
    return;
  }
  if (samplesKb.length < MIN_USABLE_SAMPLES) {
    failSetup(
      `too few samples collected (${samplesKb.length}); the process likely exited early`,
      { binary: BINARY_PATH },
    );
    return;
  }

  const expectedSamples = Math.floor(SAMPLE_WINDOW_MS / SAMPLE_INTERVAL_MS);
  printResult({
    ok: true,
    script: "idle-memory.mjs",
    ...hostContext(),
    binary: BINARY_PATH,
    sampleIntervalMs: SAMPLE_INTERVAL_MS,
    sampleWindowMs: SAMPLE_WINDOW_MS,
    incomplete: samplesKb.length < expectedSamples,
    workingSetKb: summarize(samplesKb),
    scenario: "idle shell, no account configured, no mailbox data loaded",
    limitations: [
      "Mem Usage from tasklist is OS-reported Working Set in KB, not private/committed bytes",
      "measures the empty shell, not a populated inbox",
      "dev-mode (pnpm tauri dev) numbers are not comparable to this release-binary measurement",
    ],
  });
}

await main();
