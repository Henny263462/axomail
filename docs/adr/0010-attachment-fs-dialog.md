# 0010. Attachments: file dialogs for path selection, Rust `std::fs` for disk IO

Date: 2026-07-19
Status: accepted

## Context

Wave 3 adds attachments to the reading and compose surfaces (clusters S1 and
C1, tracks 3.5 and 3.6): saving a received attachment to disk, opening it,
save-all, save-as-`.eml`, attaching files on send, and later inline `cid:`
images. Every one of these needs two OS capabilities the app has never had:

1. **A native file dialog** — an "open file" picker (choose files to attach) and
   a "save file" picker (choose where an attachment or `.eml` is written). The
   app previously granted only `opener:allow-open-url` for the OAuth consent
   screen (ADR-0009); it can open a browser to an HTTPS URL and nothing else.
2. **Disk read/write** — reading the bytes of a file the user chose to attach,
   and writing an attachment's bytes to a user-chosen destination.

These are a reviewable capability escalation, so — as with the OAuth opener in
ADR-0009 — they are added once in the Wave 3 Stage 0 sequential step, ahead of
the fan-out, and documented here. The six Wave 3 tracks then build the picker
UI and the byte-serving commands on top.

Project rules apply: MIT-compatible licenses only (`deny.toml`), minimal
dependency count and versions verified against crates.io at adoption
(ADR-0002/0006), no secrets in the database (ADR-0001), redacted errors across
the IPC boundary (ADR-0003), and the four-layer sandboxed-iframe HTML render
(ADR-0007) must stay intact. The security question is how much filesystem reach
to hand the webview: a broad fs scope would let any script in the render surface
read or overwrite arbitrary user files, which is exactly the blast radius a mail
client must not expose to message-derived content.

## Decision

### Plugins: `tauri-plugin-dialog` 2.7.2 and `tauri-plugin-fs` 2.5.1

Both are official Tauri 2 plugins from the same `tauri-apps/plugins-workspace`
monorepo as the already-adopted `tauri-plugin-opener` (`Apache-2.0 OR MIT`,
`rust-version` 1.77.2). Versions were verified against crates.io on 2026-07-19
and resolve against the workspace `tauri` 2.11.5 without bumping it. They are
pinned in `[workspace.dependencies]` and consumed by `apps/desktop/src-tauri`;
both are registered in `lib.rs` (`tauri_plugin_dialog::init()`,
`tauri_plugin_fs::init()`). No optional features are enabled (`fs`'s
`watch`/`notify` and `dialog`'s `xdg-portal` stay off).

`tauri-plugin-dialog` depends on `tauri-plugin-fs` regardless (it returns
`tauri_plugin_fs::FilePath` and integrates with its scope), so `fs` enters the
graph either way; registering it explicitly keeps its command surface available
for a future, separately-reviewed webview grant without another ADR-level dep
change.

### The rule: dialog picks the path, Rust `std::fs` does the IO

**File contents are read and written only by Rust commands using
`std::fs`, against a path the user chose through the dialog plugin.** The bytes
never transit an ambient webview filesystem API. Concretely, for the Wave 3
tracks:

- The frontend calls the dialog plugin to obtain a user-chosen path (open →
  files to attach; save → destination for an attachment or `.eml`).
- That path is passed to a `#[tauri::command]` whose `*_impl` does the
  `std::fs` read/write in the Rust backend, applying `sanitize_attachment_filename`
  on the way out and sourcing bytes from the local store, not from the webview.

### Capability grant: dialog only, no fs scope

`capabilities/default.json` adds exactly two permissions:

```json
"dialog:allow-open",
"dialog:allow-save"
```

`dialog:allow-open` and `dialog:allow-save` enable only the open-file and
save-file pickers. We deliberately do **not** grant `dialog:default` (which also
enables the `ask`/`confirm`/`message` alert dialogs) — those are not needed.

**No `fs:*` permission is granted at all.** The webview therefore has no
standing filesystem scope: it cannot read or write any path through the fs
plugin, whether or not a dialog "authorized" it. This is the narrowest workable
form — the dialog returns a path, and Rust (outside the ACL-gated webview
command surface) performs the IO. A user picking a file in the OS dialog is the
consent gesture; no persistent scope is retained afterward.

### CSP and the HTML render are untouched

Attachments need no CSP change: dialogs and file IO are IPC commands, not
network fetches or new script/style/img origins. `tauri.conf.json`'s
`connect-src`/`img-src`/`script-src` stay exactly as ADR-0009 left them, and the
sandboxed-iframe HTML render (ADR-0007) is unchanged. Inline `cid:` image
support (track 3.6) is a sanitizer-level change scoped by that track and its own
ADR-0007 update; it does not relax CSP here and is out of scope for this record.

## Consequences

- The app can now present native open/save-file dialogs and read/write disk
  files — but only through Rust `std::fs` against dialog-chosen paths. The
  webview gained a file-picker, not filesystem access.
- New crates in `Cargo.lock`: `tauri-plugin-dialog` and `tauri-plugin-fs` (both
  already partly present transitively via the Tauri stack). All licenses are on
  the `deny.toml` allow-list; `cargo deny check` gates regressions.
- A track that genuinely needs the webview to touch the filesystem directly
  (rather than via a Rust command) must add a scoped `fs:*` permission in a new,
  separately-reviewed capability entry with its own justification — it is not
  pre-granted here.
- The escalation is confined: the reviewable surface is two dialog permissions,
  and the byte paths live in auditable Rust commands, not in webview script.
