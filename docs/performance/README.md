# Performance

Development targets (from the project brief), treated as measurable goals —
**none of these are claimed as achieved**:

| Goal | Target | Status |
| ---- | ------ | ------ |
| Warm start to visible main window | < 1 s | not yet measured (window-exists proxy only, see 2026-07-18-initial.md — first paint still needs a dedicated measurement) |
| Local inbox usable without network | immediate | not implemented |
| Smooth scrolling | ≥ 50,000 indexed messages | not implemented |
| Idle memory | constant, documented | empty-shell baseline only, see 2026-07-18-initial.md — populated-inbox measurement pending phase 4 |
| Sync memory | bounded (pagination/streaming) | not implemented |

## Method

Measurement scripts live in [`scripts/bench/`](../../scripts/bench/README.md)
(Node.js, no new dependencies) and read already-built artifacts — the
release desktop binary and the production frontend bundle — rather than
building anything themselves:

| Script | Measures |
| ------ | -------- |
| `startup.mjs` | cold/warm start latency (tasklist window-detection proxy) |
| `db-scale.mjs` | paginated list query latency at 50,000 synthetic messages |
| `bundle-size.mjs` | release binary size; frontend JS/CSS raw + gzip size |
| `idle-memory.mjs` | idle Working Set of the empty shell over 30s |

Each prints one JSON result to stdout and exits non-zero on setup
failure; see `scripts/bench/README.md` for prerequisites, exact method,
and the limitations of each proxy measurement (most are Windows-only
today, matching the project's current desktop target). Further scripts
(inbox open with real data, message load, list scrolling, local search,
sync memory, 1k/10k message sync, large MIME parsing, large attachments)
are added alongside the features they measure, starting in phase 2/3/4
as each becomes exercisable end-to-end.

**Rule: results are recorded only as dated files in this directory, and
only from a script actually run on real hardware.** No performance
number appears in `README.md`, marketing text, or a PR description
before a reproducible measurement exists here. Never record a projected,
estimated, or "should be roughly" number — only verified script output.

### Recording a new measurement

1. Build what the script needs (see `scripts/bench/README.md`
   prerequisites) and run the script.
2. Create `docs/performance/YYYY-MM-DD-<slug>.md` using this template:

   ```markdown
   # <slug> — YYYY-MM-DD

   ## Machine

   - OS: <e.g. Windows 11 Pro, build NNNNN>
   - CPU: <model>
   - RAM: <amount>
   - Build: <git commit the binary/dist were built from>

   ## <script-name>.mjs

   \`\`\`json
   <exact stdout JSON from the script run>
   \`\`\`

   Notes: <anything relevant: was this a dev machine, background load,
   which code path ran (e.g. sqlite3 CLI vs node:sqlite fallback), etc>
   ```

3. Add a row to the Measurements section below linking the new file.
   Do not edit numbers into this README directly — only link to the
   dated file that contains the raw JSON.

## Measurements

| Date | File | Machine | Notes |
| ---- | ---- | ------- | ----- |
| 2026-07-18 | [2026-07-18-initial.md](2026-07-18-initial.md) | Windows 11 Pro dev machine | First real run of all four `scripts/bench/` scripts; see file for exact JSON and caveats. |
