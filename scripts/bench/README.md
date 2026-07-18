# Performance measurement scripts

Node.js scripts that measure axomail's real, built artifacts — the
release desktop binary and the production frontend bundle. No new npm
dependencies: everything here uses `node:child_process`, `node:fs`,
`node:perf_hooks`, `node:zlib`, `node:os`, and (for `db-scale.mjs`) the
built-in `node:sqlite` module. Shared helpers live in `lib/`.

This is tooling, not product code. It is not covered by the Rust
`MailError`/ADR rules that govern `crates/` — but it follows the same
honesty rule as the rest of the project: **no placeholder numbers, no
invented results.** Every script either prints a real measurement or
exits non-zero.

## The rule: results live only in `docs/performance/`

Running a script prints a JSON result to stdout. That JSON is **not**
itself a recorded measurement — recording means copying a real, just-run
result into a dated file under `docs/performance/` (e.g.
`docs/performance/2026-07-18-initial.md`), with the hardware and OS
noted. Nothing here should ever be pasted into `README.md`, marketing
copy, or a PR description as a claimed number unless it came from such a
dated file. See `docs/performance/README.md` for the template and the
full rule.

## Prerequisites

All four scripts read already-built artifacts; none of them build
anything. Before running them:

```sh
cargo build --release -p axomail-desktop
pnpm build
```

`startup.mjs`, `idle-memory.mjs`, and `db-scale.mjs`'s `tasklist`/
`taskkill` usage are Windows-only today (the project's only supported
desktop target so far); each exits with a clear `ok: false` JSON result
and a non-zero exit code on other platforms instead of guessing.
`bundle-size.mjs` and `db-scale.mjs`'s `node:sqlite` fallback are
platform-independent.

## Running

```sh
node scripts/bench/startup.mjs
node scripts/bench/db-scale.mjs
node scripts/bench/bundle-size.mjs
node scripts/bench/idle-memory.mjs
```

or via the root `package.json` scripts:

```sh
pnpm bench:startup
pnpm bench:db
pnpm bench:size
pnpm bench:memory
```

Every script prints one JSON object to stdout and exits `0` on success.
On any setup failure (binary missing, dist missing, process would not
spawn, platform unsupported, window never appeared, too few samples
collected) it prints a `{ "ok": false, "error": "..." }` JSON object and
exits non-zero — check the `error` field rather than parsing stderr.

## What each script measures

### `startup.mjs` — cold/warm start proxy

Spawns `target/release/axomail-desktop.exe`, polls `tasklist` until the
process has a window title, then kills the whole process tree
(`taskkill /T /F`, so WebView2 helper processes are cleaned up too).
Runs `N=5` times for a "cold" group (each preceded by a 2s cooldown) and
`N=5` times for a "warm" group (launched back-to-back), reporting
min/median/max per group in milliseconds.

**Limitations (read before trusting these numbers):**

- Window detection is a `tasklist` Window Title poll, not first paint.
  It tells you when the OS registered a titled top-level window, not
  when the Vue app has finished mounting or rendering visible content.
  Measuring true first-paint needs a frame-level capture (an OS
  compositor trace, or an in-app timestamp emitted once the frontend
  mounts) — do that manually, or add it as an app-side event later.
- There is no user-space way to force a true cold OS file-cache
  eviction on Windows without a reboot or admin tooling (e.g. RAMMap).
  "cold" here means "first launch of this bench run, with a cooldown
  pause before it", not "first launch after a reboot". On a machine
  that ran the binary recently — including moments earlier, by this
  very script — both groups may be OS-cache-warm. Treat the "cold"
  numbers as an upper-bound proxy.

### `db-scale.mjs` — paginated list query latency at 50k rows

Generates one `.sql` file (schema + batched `INSERT`s) for 50,000
synthetic messages in a single mailbox, using a schema that is a
simplified subset of the production `messages` table (see
`crates/mail-storage/migrations/0002_messages_and_attachments.sql`) —
only the columns and index (`idx_messages_mailbox_date`) the list query
needs. `message_recipients` and `attachments` are not populated; this is
a query-shape approximation, not a full schema replica.

Two engines can execute that file, and the JSON result's `path` field
always says which one ran:

- **`sqlite3` CLI**, if found on `PATH`: the file's SQL is piped to it
  over stdin, and query timing prefers the CLI's own `.timer on` output
  (`Run Time: real ...`), falling back to wall-clock around the process
  if a given `sqlite3` build formats that output differently.
- **`node:sqlite` fallback** otherwise (Node's built-in `DatabaseSync`
  — no `sqlx`, no external CLI, no new npm dependency): the same file is
  read and executed with `db.exec()`, and query timing uses
  `node:perf_hooks` directly. This is the path that ran during
  development of this tooling (no `sqlite3` CLI on that machine) — see
  `docs/performance/2026-07-18-initial.md`.

Measures a first page (`OFFSET 0`) and a deep page (`OFFSET 49950`, the
last page — the worst case for `OFFSET`-based pagination) at `LIMIT 50`,
`N=5` iterations each, reporting min/median/max in milliseconds. The
query is wrapped as `SELECT count(*) FROM (<paginated query>)` so both
engines measure the identical query plan without needing to print or
parse 50 rows of synthetic text per call.

**Report these as approximations, not precise query costs** — each
`sqlite3` CLI invocation pays fresh process-start and DB-open overhead,
and `N=5` is too small a sample for statistical confidence. They are
useful for spotting order-of-magnitude regressions (e.g. "deep-page
pagination got 50x slower"), not for tight before/after comparisons.

The generated database is written to a temporary directory and deleted
after the run; pass `--keep-db` to keep it (path printed in the result).

### `bundle-size.mjs` — binary and frontend bundle sizes

Reports `target/release/axomail-desktop.exe` size, plus every `.js`/
`.css` file under `apps/desktop/dist` with raw and gzip size (gzip
computed locally with `node:zlib`, `level: 9` — this approximates but
will not exactly match a given HTTP server's or CDN's gzip output).
Read-only: builds nothing.

### `idle-memory.mjs` — idle Working Set of the empty shell

Launches the release binary, waits for its window (same `tasklist`
proxy as `startup.mjs`), then samples "Mem Usage" (Working Set, in KB)
every 2s for 30s of idle wall-clock time with no user interaction,
reports median/max, then kills the process tree.

**Limitations:**

- `tasklist`'s Mem Usage is the OS-reported Working Set in KB, not a
  private/committed-bytes figure and not an in-process allocator
  measurement.
- This measures the **empty shell** — no account configured, no mailbox
  data loaded. It is not idle memory of a populated inbox; that needs
  its own dated measurement once phase 4 sync/storage is wired
  end-to-end.
- **`pnpm tauri dev` numbers are not comparable to this script.** Dev
  mode runs an unminified frontend against a live dev server inside the
  WebView plus devtools hooks, which inflates memory well beyond a
  released build. Only release-binary numbers belong in
  `docs/performance/`.

## Locale note

`tasklist`'s column headers and status text (e.g. "N/A") are localized
by Windows display language. `lib/win-tasklist.mjs` never matches on
that text — it reads fixed CSV column *positions*, which `tasklist`
documents as stable across locales, and detects "no window yet" by
tracking the raw Window Title string rather than comparing it against a
hardcoded "N/A" spelling.
