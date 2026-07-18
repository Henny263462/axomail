#!/usr/bin/env node
/**
 * Generates a synthetic 50,000-row SQLite message table and measures
 * paginated list-query latency at the first page and a deep page (last
 * page, worst case for OFFSET-based pagination).
 *
 * Run: node scripts/bench/db-scale.mjs
 * Or:  pnpm bench:db
 *
 * The schema is a deliberately simplified subset of the production
 * `messages` table (see crates/mail-storage/migrations/
 * 0002_messages_and_attachments.sql): only the columns the list query
 * touches, plus the same `idx_messages_mailbox_date` index. It is a
 * synthetic approximation for query-shape benchmarking, not a full
 * schema replica — `message_recipients` and `attachments` are not
 * populated, and only one mailbox is used.
 *
 * Generation always produces one `.sql` file (schema + batched INSERTs)
 * on disk first. Two engines can execute it, and the result JSON always
 * says which one ran:
 *   - `sqlite3` CLI, if found on PATH: the file's contents are piped to
 *     it over stdin, and query timing prefers the CLI's own `.timer on`
 *     output (falling back to wall-clock around the process if a given
 *     sqlite3 build's timer text does not match the expected format).
 *   - node:sqlite (Node's built-in `DatabaseSync`, no sqlx, no CLI,
 *     no new npm dependency) otherwise: the same file is read and run
 *     via `db.exec()`, and query timing uses node:perf_hooks directly.
 *
 * Every latency reported here is a small-N wall-clock approximation
 * (process spawn overhead dominates the CLI path in particular) — treat
 * these as order-of-magnitude numbers, not precise query costs.
 */
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";
import { failSetup, hostContext, printResult } from "./lib/report.mjs";
import { summarize } from "./lib/stats.mjs";

const ROW_COUNT = 50_000;
const BATCH_SIZE = 500;
const PAGE_SIZE = 50;
const FIRST_PAGE_OFFSET = 0;
const DEEP_PAGE_OFFSET = ROW_COUNT - PAGE_SIZE;
const QUERY_ITERATIONS = 5;
const SEED = 0x2a5f3c11;

/** Deterministic PRNG (mulberry32) so repeated runs generate identical
 * synthetic data — not a security-relevant use of randomness. */
function mulberry32(seed) {
  let a = seed;
  return function next() {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function sqlQuote(value) {
  return value.replace(/'/g, "''");
}

/** Builds the schema + batched-INSERT SQL text for `rowCount` synthetic
 * messages in a single mailbox, wrapped in one transaction. */
function generateSql(rowCount) {
  const rand = mulberry32(SEED);
  const baseUnix = 1_700_000_000;
  const lines = [
    "BEGIN;",
    "CREATE TABLE messages (",
    "  id INTEGER PRIMARY KEY,",
    "  mailbox_id INTEGER NOT NULL,",
    "  remote_kind TEXT,",
    "  remote_uid INTEGER,",
    "  subject TEXT,",
    "  date_unix INTEGER,",
    "  flag_seen INTEGER NOT NULL DEFAULT 0,",
    "  preview TEXT,",
    "  size_bytes INTEGER",
    ") STRICT;",
    "CREATE INDEX idx_messages_mailbox_date ON messages (mailbox_id, date_unix DESC, id DESC);",
  ];

  let batch = [];
  for (let i = 0; i < rowCount; i += 1) {
    const seen = rand() < 0.8 ? 1 : 0;
    const sizeBytes = 512 + Math.floor(rand() * 500_000);
    const subject = sqlQuote(`Synthetic message ${i}`);
    const preview = sqlQuote(`Preview text for message ${i}: lorem ipsum dolor sit amet.`);
    batch.push(`(1,'imap',${i + 1},'${subject}',${baseUnix + i},${seen},'${preview}',${sizeBytes})`);
    if (batch.length === BATCH_SIZE || i === rowCount - 1) {
      lines.push(
        "INSERT INTO messages (mailbox_id, remote_kind, remote_uid, subject, date_unix, flag_seen, preview, size_bytes) " +
          `VALUES ${batch.join(",")};`,
      );
      batch = [];
    }
  }
  lines.push("COMMIT;");
  return `${lines.join("\n")}\n`;
}

/** The paginated list query, wrapped in an outer COUNT(*) so both the
 * CLI and node:sqlite paths can measure the identical query plan
 * (index scan + LIMIT/OFFSET skip) without needing to suppress or parse
 * 50 rows of synthetic text per call. */
function pageCountQuery(offset) {
  return (
    "SELECT count(*) FROM (SELECT id, subject, date_unix FROM messages " +
    `WHERE mailbox_id = 1 ORDER BY date_unix DESC, id DESC LIMIT ${PAGE_SIZE} OFFSET ${offset})`
  );
}

function detectSqliteCli() {
  const result = spawnSync("sqlite3", ["-version"], { encoding: "utf8", windowsHide: true });
  return !result.error && result.status === 0;
}

/** Runs the CLI path: pipe the SQL file to `sqlite3` to create the DB,
 * then run both page queries `QUERY_ITERATIONS` times each in one
 * session with `.timer on`, parsing `Run Time: real <seconds>` lines.
 * Falls back to per-call wall-clock timing if that text isn't found. */
function runWithSqliteCli(dbPath, sqlFileContent) {
  const genStart = performance.now();
  const genResult = spawnSync("sqlite3", [dbPath], {
    input: sqlFileContent,
    encoding: "utf8",
    windowsHide: true,
  });
  const generationMs = performance.now() - genStart;
  if (genResult.error || genResult.status !== 0) {
    throw new Error(`sqlite3 CLI failed to generate the database: ${genResult.stderr || genResult.error}`);
  }

  const queryScript =
    ".timer on\n" +
    `${pageCountQuery(FIRST_PAGE_OFFSET)};\n`.repeat(QUERY_ITERATIONS) +
    `${pageCountQuery(DEEP_PAGE_OFFSET)};\n`.repeat(QUERY_ITERATIONS);
  const queryStart = performance.now();
  const queryResult = spawnSync("sqlite3", [dbPath], {
    input: queryScript,
    encoding: "utf8",
    windowsHide: true,
  });
  const totalQueryWallMs = performance.now() - queryStart;
  if (queryResult.error || queryResult.status !== 0) {
    throw new Error(`sqlite3 CLI failed to run page queries: ${queryResult.stderr || queryResult.error}`);
  }

  const timerMatches = [...queryResult.stdout.matchAll(/Run Time: real ([\d.]+)/g)].map(
    (m) => Number.parseFloat(m[1]) * 1000,
  );
  let firstPageMs;
  let deepPageMs;
  let timingSource;
  if (timerMatches.length === QUERY_ITERATIONS * 2) {
    firstPageMs = timerMatches.slice(0, QUERY_ITERATIONS);
    deepPageMs = timerMatches.slice(QUERY_ITERATIONS);
    timingSource = "sqlite3 CLI .timer";
  } else {
    // Some sqlite3 builds format .timer output differently; fall back
    // to an even split of total wall-clock time across all calls.
    const approxEach = totalQueryWallMs / (QUERY_ITERATIONS * 2);
    firstPageMs = Array(QUERY_ITERATIONS).fill(approxEach);
    deepPageMs = Array(QUERY_ITERATIONS).fill(approxEach);
    timingSource = "sqlite3 CLI wall-clock fallback (.timer output format not recognized)";
  }

  return { generationMs, firstPageMs, deepPageMs, timingSource, tool: "sqlite3 CLI" };
}

/** Runs the fallback path: read the same generated SQL file and execute
 * it via node:sqlite's `DatabaseSync.exec`, timing everything with
 * node:perf_hooks. */
async function runWithNodeSqlite(dbPath, sqlFileContent) {
  let DatabaseSync;
  try {
    ({ DatabaseSync } = await import("node:sqlite"));
  } catch {
    throw new Error("neither the sqlite3 CLI nor node:sqlite is available on this Node build");
  }

  const genStart = performance.now();
  const db = new DatabaseSync(dbPath);
  db.exec(sqlFileContent);
  const generationMs = performance.now() - genStart;

  const timeQuery = (offset) => {
    const stmt = db.prepare(pageCountQuery(offset));
    stmt.get(); // warm-up: excluded from reported samples
    const samples = [];
    for (let i = 0; i < QUERY_ITERATIONS; i += 1) {
      const t0 = performance.now();
      stmt.get();
      samples.push(performance.now() - t0);
    }
    return samples;
  };

  const firstPageMs = timeQuery(FIRST_PAGE_OFFSET);
  const deepPageMs = timeQuery(DEEP_PAGE_OFFSET);
  db.close();

  return {
    generationMs,
    firstPageMs,
    deepPageMs,
    timingSource: "node:perf_hooks (node:sqlite fallback)",
    tool: "node:sqlite (bundled with Node, no sqlx, no CLI)",
  };
}

async function main() {
  const keepDb = process.argv.includes("--keep-db");
  const tmpDir = mkdtempSync(path.join(tmpdir(), "axomail-bench-db-"));
  const sqlFilePath = path.join(tmpDir, "generate.sql");
  const dbPath = path.join(tmpDir, "bench.db");

  try {
    const sqlContent = generateSql(ROW_COUNT);
    writeFileSync(sqlFilePath, sqlContent, "utf8");

    const cliAvailable = detectSqliteCli();
    let outcome;
    try {
      outcome = cliAvailable
        ? runWithSqliteCli(dbPath, sqlContent)
        : await runWithNodeSqlite(dbPath, sqlContent);
    } catch (err) {
      failSetup(`database generation or query measurement failed: ${err.message}`, {
        cliAvailable,
      });
      return;
    }

    const dbSizeBytes = statSync(dbPath).size;
    printResult({
      ok: true,
      script: "db-scale.mjs",
      ...hostContext(),
      path: outcome.tool,
      sqlFile: sqlFilePath,
      rowCount: ROW_COUNT,
      pageSize: PAGE_SIZE,
      unit: "ms",
      dbSizeBytes,
      generationMs: Math.round(outcome.generationMs * 100) / 100,
      timingSource: outcome.timingSource,
      firstPage: { offset: FIRST_PAGE_OFFSET, ...summarize(outcome.firstPageMs) },
      deepPage: { offset: DEEP_PAGE_OFFSET, ...summarize(outcome.deepPageMs) },
      note: "micro-latencies are small-N approximations; see script header for method",
      dbKept: keepDb,
      dbPath: keepDb ? dbPath : null,
    });
  } finally {
    if (!keepDb) {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  }
}

await main();
