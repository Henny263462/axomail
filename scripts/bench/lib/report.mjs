/**
 * Shared stdout/exit-code contract for every bench script:
 * always print exactly one JSON object to stdout, exit 0 on success and
 * non-zero on setup failure (missing binary, missing dist, spawn failure,
 * unsupported platform, etc).
 */
import process from "node:process";

/** Prints `result` as pretty JSON to stdout. Use for successful runs. */
export function printResult(result) {
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}

/**
 * Prints a `{ ok: false, error, ... }` JSON object to stdout and exits
 * with status 1. `extra` may add non-sensitive diagnostic fields (never
 * include credentials, server responses, or other raw protocol data —
 * these scripts only ever touch local synthetic data, but keep the same
 * discipline as the Rust error strategy, ADR-0003).
 */
export function failSetup(error, extra = {}) {
  printResult({ ok: false, error, ...extra });
  process.exitCode = 1;
}

/** Returns a small object describing the machine this script ran on, for
 * embedding in results so dated files in docs/performance/ carry context. */
export function hostContext() {
  return {
    platform: process.platform,
    arch: process.arch,
    nodeVersion: process.version,
  };
}
