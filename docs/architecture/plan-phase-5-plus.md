# Phase 5+ — Thunderbird-parity delivery plan

Goal: upgrade axomail from the MVP (phases 1–4) to a mail client on par with
desktop Thunderbird's mail feature set. Baseline and gap analysis:
[current-state-2026-07-19.md](current-state-2026-07-19.md). The priorities from
the kickoff spec remain binding: correctness > security > speed > memory >
offline > usability > feature scope.

This plan was adversarially reviewed (dependency/conflict, Thunderbird-parity,
and feasibility lenses) on 2026-07-19; all confirmed findings are incorporated
below (offline store, trash/expunge/compact, HTTP-client + opener
prerequisites, QRESYNC rescope, hotspot pre-splits, migration renumbering).

Execution model: waves. Each wave = a sequential **Stage 0** (shared plumbing
and hotspot pre-splits in the main tree, committed before fan-out), then
parallel tracks implemented by independent agents in isolated git worktrees,
merged sequentially, then full verification, a multi-agent adversarial review,
fixes, and one commit per wave.

## Conflict rules (binding for every track)

- **Migrations** are pre-assigned in the table below. Never renumber, never
  touch shipped migrations (head at plan time: `0005`). Unused reservations
  simply stay unused (gaps are fine).
- **Shared hotspots** (`lib.rs` `generate_handler![]`, `dto.rs` re-exports,
  `App.vue`, `composition.rs`, `provider.rs`, `pop3_sync.rs`,
  `capabilities/default.json`): tracks may append; conflicts are resolved at
  integration as a union of additions. Files near the 400-line gate are
  pre-split in Stage 0 **before** tracks fan out (Wave 1: `SettingsDialog.vue`
  379/400, `App.vue` 343/400; Wave 3: `ComposerDialog.vue` 351/400,
  `commands/compose.rs` 332/400).
- Each track ships tests, respects the 400-line file gate, and must pass
  targeted verification (fmt + clippy + `cargo test -p` touched crates +
  `pnpm typecheck && pnpm test` when frontend is touched) before hand-back.
- Work lands on a branch `wave<N>/t<K>-<slug>`; the integrator merges,
  resolves, re-verifies the union, reviews, commits.

## Migration ledger

| Nr | Owner | Content |
|---|---|---|
| 0006 | 1.1 | `messages`: message_id / in_reply_to / references + indexes |
| 0007 | 1.2 | `accounts`: auth_mechanism CHECK, oauth client config (non-secret) |
| 0008 | 1.5 | `identities` (incl. signature, reply-to, composition prefs, Sent/Drafts/Templates/Archive destinations, archive granularity) |
| 0009 | 1.6 | remote-content allow list (per-sender) |
| 0010 | W2-S0 | `pending_ops` queue; `sync_jobs` kind CHECK + account-scoped dedup |
| 0011 | 2.4 | `tags`, `message_tags` |
| 0012 | 2.2 | `mailboxes`: hierarchy (parent, delimiter, subscribed, counts) |
| 0013 | 2.3 | per-folder offline policy; raw-store bookkeeping |
| 0014 | W3 | reserved (expected unused) |
| 0015 | 4.1 | contacts |
| 0016 | 4.2 | filters |
| 0017 | 4.3 | junk (bayes tokens, per-message score) |
| 0018 | 4.4 | calendar-invitation state |
| 0019 | 5.1 | pgp key metadata (secrets in keychain) |
| 0020 | 5.2 | saved searches |
| 0021 | 5.5 | polish leftovers |

## Wave 1 — Independent foundations

**Stage 0 (sequential, before fan-out):**
1. Split `SettingsDialog.vue` into per-section child components with a section
   registry so each later track adds an isolated file.
2. Extract dialog orchestration from `App.vue` (composable + `DialogHost`) to
   create ≥100 lines of headroom; later tracks register dialogs there.
3. Prerequisites for OAuth2: add `reqwest` (rustls backend, matching the
   existing rustls 0.23/ring stack, `default-features = false`) and `base64`
   as workspace deps; register `tauri-plugin-opener` + capability entry
   (system-browser open only); `cargo deny check`; ADR-0009 documenting HTTP
   client choice + opener capability.
4. Full verification; commit.

**Stage 0 registration recipes (delivered by items 1–2):**

*Add a settings section* — create
`apps/desktop/src/components/settings/<Name>Section.vue` as a `<script setup>`
component whose only prop is `context: SettingsSectionContext` (from
`components/settings/context.ts`): read the shared fields, push edits to the
two batch-saved general fields via `context.set*`, and for autonomous
settings persist through an `api/*` call and report outcomes via
`context.reportError` / `context.reportStatus`. Then append one entry
`{ id, title, component }` to the array in `components/settings/sections.ts`.
The dialog shell renders it automatically; `SettingsDialog.vue` stays
untouched (one new file + one array entry).

*Add an app-level dialog* — create the dialog SFC (copy the overlay +
`role="dialog"` + `aria-modal` + Tab focus-trap + Escape pattern), add its
name and payload type to `AppDialogMap` in `components/layout/dialogs.ts`,
and add one `v-if="dialogs.isOpen('<name>')"` branch (plus any `@event`
re-emits) to `components/layout/DialogHost.vue`. `App.vue` opens it with a
single `dialogs.open('<name>', payload)` call and, only if it needs a
post-action, handles the re-emitted event — the mount itself costs `App.vue`
zero lines.

| Track | Cluster | Scope | Migration |
|---|---|---|---|
| 1.1 | F1 Threading identity | Parse + persist Message-ID / In-Reply-To / References (parser model fields, columns + indexes, ingest, `MESSAGE_COLUMNS`), lazy backfill from cached raw on detail open, expose in detail DTO. | 0006 |
| 1.2 | E3 OAuth2 | Auth-mechanism enum (`Password` \| `OAuth2`) in core + storage; XOAUTH2 SASL for IMAP (`authenticate()`) + SMTP (`Credentials::new_xoauth2`); access/refresh tokens in keychain only; refresh flow via reqwest; configurable client id + loopback `TcpListener` redirect + system-browser consent via opener; setup-dialog OAuth path + connection test. Owns `AccountSetupDialog.vue`. | 0007 |
| 1.3 | U1 Dark mode | `data-theme` wiring, settings toggle (system/light/dark), `prefers-color-scheme`, persisted setting. | — |
| 1.4 | U2 Activity manager | Commands over existing `list_by_state`/`cancel`/`cancel_all`, activity panel, fix cancelled-job `Permanent` misclassification, cancel buttons in sync status UI. | — |
| 1.5 | C3 Identities & send-later | `identities` repo + CRUD commands + settings section; From-picker + signature insertion (plain text) + reply-to in composer; per-identity composition prefs (reply above/below, signature placement, forward mode default) and folder destinations (consumed by Waves 3); send-later via outbox `next_attempt_unix` + scheduler promotion + UI. | 0008 |
| 1.6 | S2 Remote content + scam bar | Parameterized sanitize policy (allow http(s) img when approved), per-sender/per-message allow store + "remote content blocked" bar (allow once / always for sender); deceptive-link heuristic (text/href mismatch, IP-URLs) → warning bar. | 0009 |
| 1.7 | U3 Account management | Settings section: list accounts, edit display name/servers, delete with typed confirm. New settings-section files only; must **not** touch `AccountSetupDialog.vue` (owned by 1.2). | — |

## Wave 2 — Sync core (staged)

**Stage 0 (sequential):** engine generalization: work-request enum on the
engine channel; new `JobKind::OpReplay` (note: `Discover` variant already
exists — wire it, don't re-add); **dedup key generalized to include
account scope** (fixes the documented `(kind, mailbox_id NULL)` cross-account
collapse); `recover()` no longer filters to `MailboxSync` only; `pending_ops`
table (ordered, per-account, message-scoped, retry state); additive-flag
contract (`+FLAGS`/`-FLAGS`) and keyword set on `MessageFlags` in core +
`SyncOutcome` flag-delta contract. Migration 0010. Engine tests via FakeWorld.

| Track | Cluster | Scope | Migration |
|---|---|---|---|
| 2.1 | F2+M3 Write-back | Op-queue executors: additive flag ops, move (gate on MOVE capability, fallback COPY + STORE `+FLAGS \Deleted` + UID EXPUNGE), delete-to-trash; seen/star propagation; star toggle UI; auto-mark-read-on-display (delay setting); bulk mark-read (selection/thread/folder/all). | — |
| 2.2 | F4 Folders | `Mailbox` hierarchy model, `Discover` reconcile repo (upsert/rename/orphan), folder CRUD + subscribe trait methods (all adapters + dispatch; POP3 → `UnsupportedCapability`), folder commands, nested sidebar tree + context menu + unread/total counts (indexed, not O(n) scan). | 0012 |
| 2.3 | Offline store | Per-folder offline policy (default: recent window), bulk body download during sync (populate raw store + `has_raw` + full-body FTS), "download now" folder command, global Work Offline toggle (reads local, queues writes/sends). Resolves the has_raw-never-set contradiction. | 0013 |
| 2.4 | F3 Tags | Tag model + repo, IMAP keyword read/write in adapter `convert.rs`, tag CRUD commands, tag chips in list/detail, tag quick filter, $Junk/$NotJunk groundwork. | 0011 |

**Stage 2 (sequential, after merge):** E1 incremental sync: CONDSTORE via
async-imap's typed API (`select_condstore`, `Fetch.modseq`, UNCHANGEDSINCE) as
the primary win; QRESYNC/VANISHED as a separately-tested sub-task via raw
commands + unsolicited-response draining, with CONDSTORE-only fallback;
UID-highwater reconciliation for folder ops. `highest_modseq` column already
exists — no migration expected.

## Wave 3 — Message actions & compose

**Stage 0 (sequential):** register `tauri-plugin-dialog` + `tauri-plugin-fs`
(scoped) + capabilities (ADR); pre-split `ComposerDialog.vue` and
`commands/compose.rs` below the gate; define the shared list selection
contract (multi-id selection over stable row identity, thread-aware).

**Stage A (parallel):**

| Track | Cluster | Scope |
|---|---|---|
| 3.1 | M1 Reply/forward (core) | Quoting builder (plain + HTML→text), reply/reply-all recipient derivation, subject Re:/Fwd: normalization, In-Reply-To/References in SMTP mime, forward inline **and** forward-as-attachment (message/rfc822), reply-above/below + signature placement + cursor position per identity prefs, answered/$Forwarded flags via op queue. Rust + composer-prefill API; UI buttons land in Stage B. |
| 3.3 | C2 Server Sent/Drafts | `append` trait method (IMAP APPEND with flags), Sent copy upload after send (matched by Message-ID, per-identity destination), drafts to server Drafts, POP3 local-only fallback. |
| 3.4 | M4 List UX | Virtualized message list, threaded conversation grouping (References chain) with expand/collapse, multi-select implementation of the Stage-0 selection contract, quick-filter bar (unread/starred/attachment/tag + text), sort + group-by-sort, configurable columns, infinite scroll via `nextCursor`. |
| 3.5 | C1 Rich compose | Dependency-free contenteditable editor (bold/italic/underline/lists/links/quote), HTML+text alternative bodies, attachments on send (dialog picker, size caps, drag-drop), HTML signatures, spellcheck enablement (webview spellcheck + language attr; platform-dependent, documented), attachment-keyword reminder, timed draft auto-save. |
| 3.6 | S1 Content out | Save/open attachment commands (sanitized filenames, dialog scope) + save-all; inline cid: images via ammonia `attribute_filter` scoped to `img@src` (data: **not** added to global `url_schemes`; adversarial tests prove data: still rejected on href; ADR-0007 update); view-source; save-as-`.eml`; body display mode (Plain Text / Sanitized HTML); all-headers toggle. |

**Stage B (sequential, after merge):**

| Track | Scope |
|---|---|
| 3.2 | M2 Actions integration: detail action bar + list context menu (reply/reply-all/forward from 3.1, move/copy to folder tree, archive per identity scheme, delete-to-trash), multi-select bulk actions, undo toast (local revert + compensating op); **Empty Trash** (typed confirm + empty-on-exit setting), IMAP EXPUNGE wiring, compact/reclaim (purge DB rows + orphaned raw `.eml` blobs). |

## Wave 4 — Breadth

| Track | Cluster | Scope | Migration |
|---|---|---|---|
| 4.1 | A1 Contacts | Contacts schema/repo/CRUD, address-book UI, recipient autocomplete, auto-collect on send, vCard import/export. | 0015 |
| 4.2 | A2 Filters | Rule model: conditions (header/body/flags/tags/size/date) → actions (move, **copy**, tag, star, seen, junk, delete-to-trash, **forward**, **reply-with-template**, priority, **stop-execution**); ordered execution; run on-arrival / manual / periodic; UI CRUD. | 0016 |
| 4.3 | A3 Junk | Bayesian classifier (per-account token store), score on ingest, mark-junk/not-junk (train + $Junk keyword + move), junk-folder routing, address-book whitelist (4.1), trust-server-junk-headers option, run-junk-controls-on-folder, mark-read-on-junk setting. | 0017 |
| 4.4 | S4 iCalendar | Typed `text/calendar` extraction, invitation card (organizer/time/agenda/conflicts-free), accept/tentative/decline → iMIP reply via 3.1 plumbing, RSVP state stored. | 0018 |
| 4.5 | U4a Shortcuts | Global keyboard map (n/r/a/f/j/k/del/u/s…), command palette (Ctrl+K), help overlay, settings listing. | — |
| 4.6 | U4b Print & import/export | Print message (print-friendly doc), export folder as mbox, import mbox/EML. | — |
| 4.7 | E2 IMAP IDLE | Persistent IDLE connection job per account (reconnect + backoff, immediate sync trigger), settings toggle, poll fallback. | — |

## Wave 5 — Heavy tail

| Track | Cluster | Scope | Migration |
|---|---|---|---|
| 5.1 | S3 OpenPGP | rPGP pinned to pure-Rust crypto features (license-vetted via cargo-deny **before** merge): key gen/import/export (secrets keychain-guarded), verify badge, decrypt inline, sign/encrypt on send, per-recipient key management UI. S/MIME: verify-only if feasible, else documented out. | 0019 |
| 5.2 | Search depth | Advanced multi-condition search builder (all/any of header/body/date/tag/attachment, folder scope) reusing the 4.2 condition model; saved searches as virtual folders in sidebar. | 0020 |
| 5.3 | Autoconfig | ISPDB/Autoconfig lookup (https only, via reqwest) + common-provider guessing in setup. | — |
| 5.4 | i18n | vue-i18n (license-checked via existing npm gate), string extraction, `en` + `de`, language setting. | — |
| 5.5 | Polish | MDN in full (request per message + DSN option in composer; honor policy never/ask/always with addressed-to-me/list conditions; $MDNSent via op queue; receipt filing), message templates (Templates role), per-folder retention, resizable panes + Classic/Vertical/Wide layouts, folder-pane view modes (All/Unified/Favorite/Tags), a11y pass. | 0021 |

## Explicitly deferred (documented non-goals for this push)

- Full calendar/tasks component (Lightning equivalent) — mail-side iMIP lands
  in 4.4; a calendar is a separate future phase.
- NNTP, RSS/feeds, chat, EWS, JMAP, Gmail label API.
- **Server-side IMAP SEARCH**: deliberate (ADR-0008 local-first); the Wave-2
  offline body store closes the unindexed-body gap that motivated it.
- Full S/MIME (beyond possible verify-only).

## Verification & review cadence (every wave)

1. Per-track targeted verification before hand-back.
2. Integrator merges tracks sequentially, resolves hotspot conflicts, re-runs
   the **full** suite: `cargo fmt --all --check` · `cargo clippy --workspace
   --all-targets --locked -- -D warnings` · `cargo test --workspace --locked` ·
   `pnpm typecheck && pnpm lint && pnpm test && pnpm build` ·
   `pnpm check:file-sizes` · `cargo deny check` when deps changed.
3. Multi-agent adversarial review over the wave diff; confirmed findings
   fixed; codex CLI second opinion when available.
4. One commit per wave on `main` (Stage 0 may commit separately); docs + ADRs
   updated with the wave.

## Status

| Wave | Status |
|---|---|
| 1 | in progress (Stage 0) |
| 2 | pending |
| 3 | pending |
| 4 | pending |
| 5 | pending |
