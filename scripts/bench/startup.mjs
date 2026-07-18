#!/usr/bin/env node
/**
 * Measures cold and warm start latency of the release desktop binary
 * (target/release/axomail-desktop.exe) by spawning it, polling until a
 * window appears, then killing the whole process tree.
 *
 * Run: node scripts/bench/startup.mjs
 * Or:  pnpm bench:startup
 *
 * Windows-only today (see docs/performance/README.md and
 * scripts/bench/README.md for the full method + limitations). The
 * headline limitations, spelled out because they affect how these
 * numbers may be used:
 *
 * 1. "Window exists" is measured via `tasklist`'s Window Title column,
 *    polled at a fixed interval. That is a proxy for "the OS created a
 *    top-level window with a title", not for first paint / the window
 *    becoming visually complete. Measuring true first-paint requires a
 *    frame-level capture (e.g. an OS compositor trace or an in-app
 *    timestamp emitted once the Vue app mounts) which is out of scope
 *    for this script — do it manually or add an app-side event later.
 * 2. There is no supported way from user space to force a true
 *    "cold" OS file-cache eviction without a reboot or admin-level
 *    tools (e.g. RAMMap). "cold" here means "first launch of this bench
 *    run, with a cooldown pause before it"; "warm" means "launched
 *    immediately after a previous run". On a machine that has run the
 *    binary recently (including by this very script, moments earlier),
 *    both groups are likely OS-cache-warm — treat "cold" numbers here
 *    as an upper-bound proxy, not a true cold-boot measurement.
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import process from "node:process";
import { performance } from "node:perf_hooks";
import { failSetup, hostContext, printResult } from "./lib/report.mjs";
import { summarize } from "./lib/stats.mjs";
import { isAlive, killTree, waitForExit, windowTitle } from "./lib/win-tasklist.mjs";

const RUNS_PER_GROUP = 5;
const WINDOW_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 25;
const EXIT_TIMEOUT_MS = 5_000;
const COLD_COOLDOWN_MS = 2_000;

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const BINARY_PATH = path.join(REPO_ROOT, "target", "release", "axomail-desktop.exe");

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Spawns the binary, polls for a window, kills it, and returns the
 * elapsed milliseconds from spawn to first observed window title.
 * Throws on any setup problem (process died, window never appeared). */
async function measureOneStart() {
  const start = performance.now();
  const child = spawn(BINARY_PATH, [], { stdio: "ignore", detached: false });
  const pid = await new Promise((resolve, reject) => {
    child.once("spawn", () => resolve(child.pid));
    child.once("error", reject);
  });

  try {
    const deadline = Date.now() + WINDOW_TIMEOUT_MS;
    let elapsedMs = null;
    while (Date.now() < deadline) {
      if (!isAlive(pid)) {
        throw new Error("process exited before a window was observed");
      }
      const title = windowTitle(pid);
      if (title !== null) {
        elapsedMs = performance.now() - start;
        break;
      }
      // eslint-disable-next-line no-await-in-loop -- intentional poll loop
      await delay(POLL_INTERVAL_MS);
    }
    if (elapsedMs === null) {
      throw new Error(`no window observed within ${WINDOW_TIMEOUT_MS}ms`);
    }
    return elapsedMs;
  } finally {
    killTree(pid);
    await waitForExit(pid, EXIT_TIMEOUT_MS);
  }
}

async function measureGroup(count, cooldownMs) {
  const samples = [];
  for (let i = 0; i < count; i += 1) {
    if (cooldownMs > 0) {
      // eslint-disable-next-line no-await-in-loop -- sequential by design
      await delay(cooldownMs);
    }
    // eslint-disable-next-line no-await-in-loop -- runs must not overlap
    samples.push(await measureOneStart());
  }
  return samples;
}

async function main() {
  if (process.platform !== "win32") {
    failSetup(
      "startup.mjs only implements the tasklist-based window-detection proxy on win32",
      { platform: process.platform },
    );
    return;
  }
  if (!existsSync(BINARY_PATH)) {
    failSetup("release binary not found; run: cargo build --release -p axomail-desktop", {
      binary: BINARY_PATH,
    });
    return;
  }

  let cold;
  let warm;
  try {
    cold = await measureGroup(RUNS_PER_GROUP, COLD_COOLDOWN_MS);
    warm = await measureGroup(RUNS_PER_GROUP, 0);
  } catch (err) {
    failSetup(`startup measurement failed: ${err.message}`, { binary: BINARY_PATH });
    return;
  }

  printResult({
    ok: true,
    script: "startup.mjs",
    ...hostContext(),
    binary: BINARY_PATH,
    runsPerGroup: RUNS_PER_GROUP,
    method: "tasklist window-title polling proxy (not true first-paint)",
    unit: "ms",
    cold: summarize(cold),
    warm: summarize(warm),
    limitations: [
      "window detection is a tasklist Window Title poll, not first paint",
      "no true cold OS-cache eviction available from user space; both groups may be cache-warm",
    ],
  });
}

await main();
