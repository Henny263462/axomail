/**
 * Windows process inspection via the built-in `tasklist`/`taskkill`
 * tools, used as a proxy for "is this process alive / does it have a
 * window yet" since Node has no portable API for either on Windows
 * without a native addon (out of scope: no new dependencies).
 *
 * `tasklist` output is locale-dependent (header names and status/N-A
 * text are translated, e.g. German "Nicht zutreffend" instead of
 * "N/A"). This module never matches on those localized strings — it
 * relies only on the fixed column *order* CSV output, which
 * `tasklist` documents as stable across locales:
 *   0 Image Name, 1 PID, 2 Session Name, 3 Session#, 4 Mem Usage,
 *   5 Status, 6 User Name, 7 CPU Time, 8 Window Title (only with /V).
 */
import { spawnSync } from "node:child_process";

const COL = {
  IMAGE_NAME: 0,
  PID: 1,
  MEM_USAGE: 4,
  WINDOW_TITLE: 8,
};

/** Parses one CSV line in the quoted, comma-separated format `tasklist
 * /FO CSV` emits (RFC 4180-ish: fields quoted, `""` is an escaped
 * quote). Returns an array of field strings, or `null` if the line does
 * not look like a quoted CSV data row (e.g. a localized "no tasks
 * found" message). */
function parseCsvLine(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith('"')) return null;
  const fields = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < trimmed.length; i += 1) {
    const ch = trimmed[i];
    if (inQuotes) {
      if (ch === '"' && trimmed[i + 1] === '"') {
        current += '"';
        i += 1;
      } else if (ch === '"') {
        inQuotes = false;
      } else {
        current += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ",") {
      fields.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  fields.push(current);
  return fields;
}

/** Runs `tasklist` filtered to one PID with verbose columns (adds
 * Window Title). Returns the parsed field array for that PID, or `null`
 * if the process is not currently running. Throws if `tasklist` itself
 * cannot be invoked (missing on non-Windows, PATH problem, etc). */
export function queryProcess(pid) {
  const result = spawnSync(
    "tasklist",
    ["/FO", "CSV", "/V", "/FI", `PID eq ${pid}`],
    { encoding: "utf8", windowsHide: true },
  );
  if (result.error) {
    throw new Error(`failed to invoke tasklist: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`tasklist exited with status ${result.status}`);
  }
  const lines = result.stdout.split(/\r?\n/);
  for (const line of lines) {
    const fields = parseCsvLine(line);
    if (fields && fields[COL.PID] === String(pid)) return fields;
  }
  return null;
}

/** True if a process with this PID currently exists. */
export function isAlive(pid) {
  return queryProcess(pid) !== null;
}

/** Current window title for this PID, or `null` if the process has no
 * window yet (or has already exited). Untranslated because it never
 * compares against the localized "N/A" placeholder text (see module
 * doc) — callers should treat an empty string the same as `null`. */
export function windowTitle(pid) {
  const fields = queryProcess(pid);
  if (!fields) return null;
  const title = (fields[COL.WINDOW_TITLE] ?? "").trim();
  return title.length > 0 ? title : null;
}

/** Current working-set ("Mem Usage") in kilobytes, or `null` if the
 * process is not running. `tasklist` renders this with a locale-specific
 * thousands separator (e.g. "24.096 K" or "24,096 K") and no decimals,
 * so stripping every non-digit character is locale-safe. */
export function memUsageKb(pid) {
  const fields = queryProcess(pid);
  if (!fields) return null;
  const digits = (fields[COL.MEM_USAGE] ?? "").replace(/[^0-9]/g, "");
  return digits.length > 0 ? Number.parseInt(digits, 10) : null;
}

/** Kills the process and its full descendant tree (Tauri/WebView2 spawn
 * helper processes) via `taskkill /T /F`. Never throws: killing a
 * process that already exited is not an error for bench purposes. */
export function killTree(pid) {
  spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
    encoding: "utf8",
    windowsHide: true,
  });
}

/** Polls until the process is gone or `timeoutMs` elapses. Returns true
 * if it confirmed the process exited. */
export async function waitForExit(pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!isAlive(pid)) return true;
    // eslint-disable-next-line no-await-in-loop -- intentional poll loop
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return !isAlive(pid);
}
