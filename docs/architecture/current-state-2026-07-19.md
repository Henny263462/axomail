# axomail — Current-State Architecture Synthesis (2026-07-19)

Consolidated from parallel subsystem reads of the whole workspace. This is the
baseline map for planning the push toward Thunderbird-level feature completeness.
It records **what exists today**, **how to extend each layer**, and **what is
missing**, grouped so a work-package can be scoped against it.

Governing ADRs referenced throughout: 0001 (protocol/storage split, no secrets in
DB), 0002 (`axomail-` package prefix), 0003 (error redaction + classification),
0004 (async traits via native `async fn` + enum dispatch, no `dyn`/`async-trait`),
0005 (browser-preview null IPC), 0006 (protocol stack), 0007 (four-layer HTML
render defense + sandboxed iframe), 0008 (local search = FTS5).

---

## 1. Current-State Feature Matrix

Legend: **Done** = implemented and wired end-to-end · **Partial** = present but
incomplete/local-only/unwired · **Missing** = no implementation.

### Accounts, auth, setup

| Feature | Status | Notes |
|---|---|---|
| Multi-account IMAP + POP3 | Done | `IncomingProtocol = Imap \| Pop3` only |
| SMTP submission (per-send connection) | Done | stateless; fresh TLS+AUTH per message |
| Manual account setup + connection test | Done | `AccountSetupDialog.vue`, `test_connection` |
| Password auth via OS keychain | Done | `KeyringSecretStore`, ref scheme `account/<id>/<dir>` |
| Autodiscovery / ISPDB / provider presets | Missing | manual config only |
| OAuth2 / XOAUTH2 (Gmail, M365) | Missing | no auth-mechanism enum at all; blocks Gmail/Outlook |
| APOP / SASL / CRAM-MD5 (POP3/IMAP incoming) | Missing | USER/PASS + IMAP LOGIN only |
| Multiple identities / aliases per account | Missing | one email per account |
| Signatures | Missing | no storage or UI |
| Account edit UI | Missing | `delete_account` exists but no UI path |
| NNTP / Local Folders / RSS / EWS / JMAP | Missing | not modeled |

### Folders / mailboxes

| Feature | Status | Notes |
|---|---|---|
| Flat mailbox list per account | Done | fixed 5-role seed on account create |
| IMAP discover (LIST + SPECIAL-USE roles) | Partial | `discover()` implemented in adapter but **never called**; no reconcile |
| Folder tree / hierarchy / delimiter | Missing | `Mailbox` has no parent/delimiter/subscription/counts |
| Folder create / rename / delete | Missing | no trait method, no command, no UI |
| Subscribe / unsubscribe / LSUB | Missing | LIST only |
| Per-folder unread + total counts | Partial | unread via O(messages) page scan in `count_unread`; no total |
| Virtual / saved-search folders | Missing | — |

### Messages: list, read, search

| Feature | Status | Notes |
|---|---|---|
| Keyset-paginated message list (newest-first) | Done | `MessageCursor`, `list_page`; UI loads first page only |
| Unified inbox (cross-account) | Done | hardwired to `role='inbox'` |
| Message detail: parse + sanitize + render | Done | sandbox iframe, ADR-0007 |
| FTS5 full-text search (all accounts) | Done | subject+correspondents at ingest, body on open |
| List virtualization | Missing | plain `v-for`, no 100k-scale |
| Multi-select / context menu / columns / sort | Missing | single-select only |
| Threading / conversation view | Missing | no Message-ID/References stored (see below) |
| Quick-filter bar (unread/starred/tag/attachment) | Missing | — |
| Infinite scroll despite `nextCursor` | Missing | only first ~50 load in UI |
| Server-side IMAP SEARCH | Missing | local FTS only (ADR-0008) |
| Advanced/faceted search builder | Missing | single global box |

### Message actions

| Feature | Status | Notes |
|---|---|---|
| Mark seen/unseen | Partial | **local-only**; never calls `provider.set_flags` — diverges across devices |
| Flag/star | Partial | `flagged` field exists in DTO/schema, **unrendered, no toggle command** |
| Reply / reply-all / forward | Missing | composer has no reply entry path; no quoting; no In-Reply-To/References |
| Move / copy to folder | Missing | `move_message` dispatched but no command/storage; no `copy_message` at all |
| Delete / move-to-Trash / expunge | Missing | `delete_message` unwired; hard delete out of scope per safety rules |
| Archive | Missing | role exists, no action |
| Mark junk / not-junk | Missing | role exists, no training, no routing |
| Tags / labels / keywords | Missing | closed 5-flag model; no IMAP keyword read/write |
| Print / save-as-EML / view-source | Missing | raw `.eml` cached but no export command/capability |
| Bulk / multi-id ops | Missing | every provider op is single-`RemoteMessageId` |

### Attachments & content

| Feature | Status | Notes |
|---|---|---|
| Attachment metadata in detail view | Done | filename/mime/size chips, display-only |
| MIME parse (multipart, encodings, RFC 2047) | Done | `axomail-parser` wrapping `mail-parser` 0.11.5 |
| HTML sanitization (ammonia, allowlist) | Done | remote content blocked+counted; all CSS stripped |
| Attachment download / open / save | Missing | bytes never cross IPC; no fs/dialog capability |
| Inline (cid:) image display | Missing (broken E2E) | cid survives sanitize but parts filtered out of DTO + no protocol handler; data: images blocked |
| Attachment persistence | Missing | `attachments` table exists, **no repo/blob store** |
| Full offline body/HTML cache | Missing | only `preview` + FTS body text; `has_raw` always 0 |
| PGP / S-MIME sign/verify/encrypt/decrypt | Missing | opaque attachments |
| iCalendar (text/calendar) invitations | Missing | falls through as attachment |
| Remote-image allow (per-sender/message) | Missing | unconditional block only |

### Compose / send

| Feature | Status | Notes |
|---|---|---|
| Plain-text compose + send via outbox | Done | outbox state machine queued→sending→sent/failed |
| Drafts (local) | Done | JSON recipient columns |
| Outbox retry (manual) | Done | `retry_outbox`, `begin_send` dup guard |
| Local Sent copy | Partial | filed locally; **no IMAP APPEND** to server Sent |
| HTML compose / rich text | Missing | textarea only |
| Attachments on send | Missing | `attachments: Vec::new()` hardcoded |
| From/identity picker, signatures | Missing | account fixed from mailbox |
| Recipient autocomplete / contacts | Missing | raw comma-split strings |
| Send-later / scheduled send | Missing | outbox has unused `next_attempt_unix` |
| Read/return receipts, DSN | Missing | — |
| Threading headers on reply (In-Reply-To/References) | Missing | `mime.rs` never writes them |

### Sync engine & background

| Feature | Status | Notes |
|---|---|---|
| Protocol-agnostic job engine (`SyncEngine`) | Done | account-lock→semaphore→claim→pump, per-pass commit |
| Retry/backoff (temporary errors) | Done | in-memory attempt counter, equal-jitter |
| Crash recovery (requeue stale + resume pending) | Done | no dead-letter/max-restart cap |
| Cancellation (cooperative token) | Partial | `cancel`/`cancel_all` exist but **unwired to any command** |
| Background scheduler (per-account interval + backoff) | Done | inbox-role targets only |
| Incremental sync cursor | Partial | UID-highwater only; `highest_modseq` reserved/unused |
| IMAP IDLE / push | Missing | poll-only, 60s floor |
| CONDSTORE / QRESYNC | Missing | flag sync = brute-force newest-500-UID window |
| Server write-back / offline op queue | Missing | local edits never replay to server |
| Message filters / rules engine | Missing | — |
| Job/activity manager UI + cancel | Missing | engine supports it, no command surface |
| Only job kind dispatched | — | `MailboxSync` only; `Discover/Fetch/Send` are dead vocabulary |

### Cross-cutting UX

| Feature | Status | Notes |
|---|---|---|
| Three-column responsive shell | Done | fixed widths, no resizable splitters |
| Sync progress events → UI + count-only OS toast | Done | single `axomail://sync` channel |
| Settings (notifications, sync interval, POP3 delete policy) | Done | entire settings surface |
| Dark mode | Partial | tokens defined, **never activated** |
| i18n | Missing | hardcoded English |
| Global keyboard shortcuts / command palette | Missing | only modal Esc/Tab |
| Contacts / address book | Missing | no storage anywhere |
| Undo (delete/move) | Missing | — |

---

## 2. Implementer Cheat Sheet — Conventions & Extension Points

Exact registration steps per extension type. Package dirs are `crates/mail-*` but
crate names carry the `axomail-` prefix (imported as `axomail_core`, etc.).

### Global rules (apply everywhere)

- **No bare `String`/`i64` in public APIs** — newtype it (ADR-0001). Local IDs via
  `define_local_id!` in `crates/mail-core/src/id.rs` (must be `Copy`,
  `serde(transparent)`). Validated value types get `::new`/`::parse ->
  Result<_, MailError::Configuration>` **and** `serde try_from` so deserialize
  can't bypass invariants (copy `ServerEndpoint`/`EmailAddress`).
- **Errors**: one taxonomy — add a variant to `MailError`
  (`crates/mail-core/src/error.rs`, `#[non_exhaustive]`) + an arm in `class()`.
  Never carry secrets/raw server text (ADR-0003); redact before constructing.
- **Provider traits are NOT object-safe** (native `async fn -> impl Future + Send`).
  Every new method must be implemented by **all** adapters + the app dispatch enum.
  No `dyn`, no `async-trait`.
- **File-size gate**: hard ceiling **400 non-blank lines** (`scripts/check-file-sizes.mjs`,
  ESLint `max-lines`); split at ~300. Tests exempt from hard fail.
- **Every change ships tests.** Rust unit tests inline `#[cfg(test)]`; integration
  tests as `crates/<crate>/tests/<name>.rs` (auto-discovered); frontend
  `apps/desktop/tests/<name>.spec.ts`. Test files opt out of unwrap lints with
  `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
- **Full local verification** (all must pass): `cargo fmt --all --check` ·
  `cargo clippy --workspace --all-targets --locked -- -D warnings` ·
  `cargo test --workspace --locked` · `pnpm typecheck && pnpm lint && pnpm test &&
  pnpm build` · `pnpm check:file-sizes`.

### Add a database migration

1. Create `crates/mail-storage/migrations/0006_<snake_desc>.sql` (next sequential
   4-digit prefix; current head is **0005**). Forward-only, **STRICT** tables,
   `ON DELETE CASCADE` for ownership edges / `SET NULL` for soft refs.
2. Auto-embedded by `sqlx::migrate!("./migrations")` in
   `crates/mail-storage/src/store.rs`; runs on next `Store::open`. **No Rust change
   needed to run it.**
3. **Never edit/renumber a shipped 0001–0005 file** — SQLx checksums abort startup
   on mismatch. No down-migrations by convention.
4. Enum-backed column? Mirror the Rust enum variants exactly in a
   `CHECK (col IN (...))`.
5. FTS change? External-content FTS5 requires coordinated table + trigger rebuild
   (see `0005_search_fts.sql` as template), not a simple ALTER.

### Add a storage repository / repo fn

1. New file `crates/mail-storage/src/<thing>.rs`: `ThingRepo<'a> { pool: &'a
   SqlitePool }` + `pub(crate) fn new(pool)`; `#[derive(sqlx::FromRow)] ThingRow` +
   `into_record()` that validates→domain type; `const THING_COLUMNS: &str` in row
   order (add a table-aliased `_M` variant for JOINs). Copy `settings.rs` (simplest)
   or `messages.rs` (transactional + FTS) as template.
2. Every query ends `.map_err(|e| map_sqlx("operation_name", e))` (value-free).
   User input always `.bind()`, never interpolated; dynamic SQL wrapped in
   `sqlx::AssertSqlSafe(...)` with compile-time fragments only.
3. Register: add `mod thing; pub use thing::{...};` to
   `crates/mail-storage/src/lib.rs` **and** a `pub fn things(&self) ->
   ThingRepo<'_>` accessor in `store.rs`.
4. Message-bearing write path? Index header text via
   `search::upsert_content(&mut tx, id, subject, &correspondents, body)` inside the
   same transaction.
5. Test with `Store::open_in_memory()`; fixtures in
   `crates/mail-storage/tests/common/mod.rs`.

### Add a core provider trait method (adapter op)

1. Add `fn op(&mut self, ...) -> impl Future<Output = Result<T, MailError>> +
   Send;` to `IncomingMailProvider` in `crates/mail-core/src/provider.rs` (the
   `+ Send` bound is load-bearing — compile-proof test lives in that file).
2. Add request/response DTOs to `crates/mail-core/src/provider_types.rs`.
3. Implement in **each** adapter's `provider.rs`:
   `crates/mail-imap/src/message_ops.rs` (small async fns using async-imap +
   `with_deadline`, gate on `has_capability(NAME)`), `crates/mail-pop3/src/…`
   (return `MailError::UnsupportedCapability { capability }` where POP3 can't
   comply). Keep pass-state-in / return-diff-out; adapters never read the DB.
4. Add a delegating arm to `AnyIncomingProvider` in
   `apps/desktop/src-tauri/src/composition.rs` **and** the `Pop3Sync` passthrough in
   `apps/desktop/src-tauri/src/pop3_sync.rs`.
5. Bound every network round-trip with `with_deadline(timeout, …)`.

> Ops the trait already declares but **nothing calls**: `discover`, `set_flags`,
> `move_message`, `delete_message`. Wiring these needs a storage method + command,
> **not** new trait plumbing.

### Add a sync job type

Bigger than it looks — the engine currently hardcodes `JobKind::MailboxSync`:

1. `JobKind` variant + `as_str`/`parse` in
   `crates/mail-storage/src/sync_jobs.rs`; migration extending the `sync_jobs.kind`
   CHECK domain.
2. Generalize the engine command channel (today `mpsc::Sender<SyncMailbox>`): both
   `recover` (~line 166) and `enqueue_job` (~line 184) in
   `crates/mail-sync/src/engine.rs` hardcode `MailboxSync`. Carry an enum of work
   requests.
3. Add a sibling executor in `crates/mail-sync/src/worker.rs` mirroring
   `run_job`'s account-lock → semaphore → `claim` → run → `finalize` and using
   `guarded` for every network call. **Commit-then-continue** durability shape is
   mandatory.
4. **Dedup key is `(kind, mailbox_id)` only** — account-scoped kinds
   (`mailbox_id NULL`) would collapse across accounts; message-scoped ops don't fit
   (`NewSyncJob` has no `message_id` column). Add a side table if needed.
5. Test via `crates/mail-sync/tests/support` (`FakeWorld`/`FakeProvider`, no
   network).

### Add a Tauri command + DTO + event

**Command** (two-layer shape, logic never in the wrapper):

1. `async fn <name>_impl(store: &Store, …) -> Result<T, MailError>` in the right
   `apps/desktop/src-tauri/src/commands/<area>.rs` (unit-tested, runtime-independent).
2. `#[tauri::command] pub async fn <name>(state: tauri::State<'_, AppState>, …) ->
   Result<Dto, String>` wrapper pulling resources from `state` accessors
   (`store()`, `secrets()`, `factory()`, `engine()`, `scheduler()`, `raw_root()`),
   ending `.map_err(map_command_error)`.
3. **Register in the `generate_handler![]` array in
   `apps/desktop/src-tauri/src/lib.rs` (lines ~78-99)** — a command is invisible
   until listed here.
4. Test: `apps/desktop/src-tauri/tests/<area>_commands.rs` against
   `Store::open_in_memory()` + `secret_stores()` + a scripted server.

**DTO**: `apps/desktop/src-tauri/src/dto/<area>.rs`, `#[serde(rename_all =
"camelCase")]`, fallible constructor routing **every id through `dto::checked_id()`**
(rejects negative / > 2^53-1). Re-export from `dto.rs`. Password-bearing inputs
hand-write `Debug` to redact.

**Event** (only `axomail://sync` exists today): add a `SyncEvent` variant in
`crates/mail-sync/src/event.rs` (serde snake_case `tag=kind`), map it in
`apps/desktop/src-tauri/src/dto/sync_event.rs`, forward in `forward_sync_events`
(`lib.rs`). For a **new** channel, define `const X: &str = "axomail://…"`, build a
Serialize DTO, `app.emit(X, dto)`, subscribe with `listen()` on the frontend.
Remember `ErrorClass` is intentionally not serde-aware — mirror it (`SyncErrorClass`).

### Add a frontend api fn / store / component

- **api fn**: thin typed wrapper in `apps/desktop/src/api/<area>.ts` over
  `safeInvoke` (errors→null) or `invokeOrThrow` (surface error string) from
  `lib/tauri.ts` — components **never** call Tauri `invoke` directly. Command name
  is the snake_case string; args a single camelCase object. Add matching DTO type to
  `apps/desktop/src/types/mail.ts` (mirror Rust camelCase; ids are `number`). Note
  every caller must handle `null` (browser-preview, ADR-0005).
- **state**: **no Pinia/Vuex** — state lives in `App.vue` (props-down/events-up).
  Cross-cutting behavior → a composable `composables/useXxx.ts` returning
  refs/computed, disposed on unmount, with monotonic sequence guards for async (copy
  `useSyncStatus`/`useSearch`).
- **top-level view**: extend `SidebarView` union in
  `components/sidebar/view.ts`, add nav button + `selectXxx` emit in
  `SidebarPane.vue`, add load/selection state + handler in `App.vue`, add the pane
  branch in `App.vue` `#list`/`#message` slots.
- **dialog/modal**: copy the overlay + `role=dialog` + `aria-modal` + Tab
  focus-trap + Escape pattern (`ComposerDialog`/`SettingsDialog`), gate with a bool
  ref in `App.vue`, mount after `<AppShell>`.
- **message action / row affordance**: reading-pane action bar → `MessageDetail.vue`
  (none today); row star/checkbox/multi-select → `MessageListItem.vue` + a selection
  set in `App.vue`; mutation → `api/messages.ts`.
- **styling**: only `var(--…)` tokens from `styles/tokens.css`; scoped styles; one
  accent color. Dark tokens already exist — enabling dark mode is just setting
  `data-theme` on root.
- **test**: `apps/desktop/tests/<name>.spec.ts` (vitest + happy-dom), `vi.mock` the
  `src/api/*` boundary, assert on aria-labels + honest empty states.

### Add an OS capability (fs / dialog / opener / print)

Register the plugin in `lib.rs`, add the permission to
`apps/desktop/src-tauri/capabilities/default.json` (today only `core:default` +
`notification:default`), tighten scope in `tauri.conf.json` (strict CSP). This is a
deliberate, security-reviewable step — required for save-as-EML, attachment export,
print. Keep the sandboxed-iframe HTML render (ADR-0007) intact.

---

## 3. Gap Clusters vs Thunderbird — Sizing & Dependencies

Sizes are rough engineering effort: **S** ≈ days, **M** ≈ 1-2 wk, **L** ≈ several
wk, **XL** ≈ multi-month / cross-cutting. "Depends on" lists **hard** prerequisites.

### Foundation clusters (unlock many others — do first)

**F1. Threading identity (Message-ID / References / In-Reply-To)** — **M**
Parse the headers (`crates/mail-parser/src/parse.rs` already has accessors; add
fields to `model.rs`), persist via a **0006 migration** adding columns to
`messages`, extend `ingest.rs`/`messages.rs`/`message_rows.rs`. Raw `.eml` is cached
→ historical rows backfillable by re-parse.
*Blocks:* threaded view, conversation grouping, correct APPEND-of-sent-copy matching,
dedup.
*Depends on:* nothing.

**F2. Server write-back + offline operation queue** — **L**
Wire the already-implemented `set_flags`/`move_message`/`delete_message`. Needs a
new job kind (§2 sync-job steps), a pending-op side table (message-scoped, doesn't
fit `(kind, mailbox_id)` dedup), per-account ordering + retry (engine provides
scaffolding). `mark_message_seen` becomes server-propagating.
*Blocks:* flag/seen sync across devices, move/copy/delete/archive/junk **as durable
server actions**, filters that act server-side.
*Depends on:* nothing structural, but sizing assumes new-job-kind plumbing.

**F3. Additive flag / keyword (tag) model** — **L**
`MessageFlags` is a closed 5-bool struct with REPLACE-form `set_flags` (clobbers
server keywords). Needs: keyword set on the core model + serde, additive STORE
(+FLAGS/-FLAGS) semantics on the trait, flag-delta contract in `SyncOutcome`,
adapter `convert.rs` read/write, a `tags` + `message_tags` schema (0006), and
`MESSAGE_COLUMNS`/`message_rows.rs` changes.
*Blocks:* tags/labels, junk-status ($Junk/$NotJunk), $Forwarded/$MDNSent, colored
labels UI.
*Depends on:* touches the same flag write-back path as F2 — sequence F2 before/with
F3.

**F4. Folder model + discovery + management** — **L**
Extend `Mailbox` (parent, delimiter, subscription, counts), call `discover()` via a
new `JobKind::Discover` executor reconciling into an upsert/rename/delete
`MailboxRepo`, add trait methods `create/delete/rename/subscribe/unsubscribe`
(all adapters + dispatch enum), fix the account-scoped dedup-key hazard, replace the
fixed 5-mailbox seed, and build the nested-tree UI (`SidebarPane.vue`) + folder
context menu.
*Blocks:* move/copy targets beyond seeded folders, virtual/saved-search folders,
per-folder retention/offline settings.
*Depends on:* new-job-kind plumbing (shared with F2).

### Message-action clusters

**M1. Reply / reply-all / forward** — **M**
Composer reply entry path + quoting/citation (`ComposerDialog.vue`,
`commands/compose.rs`), write In-Reply-To/References in `crates/mail-smtp/src/mime.rs`.
*Depends on:* **F1** (threading headers) for correct reference chains.

**M2. Move / copy / archive / delete-to-Trash** — **M** (each S once F2/F4 land)
Reading-pane action bar (`MessageDetail.vue`) + row affordances + storage move + the
wired provider ops. `copy_message` needs a new trait method (only `move_message`
exists). Hard delete stays out of scope (safety rules); move-to-Trash is the parity
path.
*Depends on:* **F2** (write-back) + **F4** (folder targets).

**M3. Flag/star toggle + tags UI** — **S** (toggle) / **M** (tags)
`flagged` field already exists unrendered — a star toggle in `MessageListItem.vue` +
`api/messages.ts` mutation is the smallest first step.
*Depends on:* **F2** (to propagate), **F3** (for tags).

**M4. Threaded / conversation view + quick-filter bar** — **L**
List-pane restructure (grouping, expand/collapse), likely forces a real store
(Pinia) instead of `App.vue`-centralized state; add virtualization here too.
*Depends on:* **F1**.

### Compose / send clusters

**C1. HTML compose + signatures + attachments-on-send** — **L**
Rich editor, `OutgoingMessage.attachments` wiring (`compose.rs`,
`provider_types.rs`), signatures storage (needs identities → C3), attachment picker
(needs fs/dialog capability).
*Depends on:* fs/dialog capability; partial on **C3**.

**C2. Server-side Sent/Drafts (IMAP APPEND)** — **M**
New `APPEND` trait method + adapter impl + wiring so the Sent copy lands on the
server and drafts sync.
*Depends on:* **F1** (match SMTP-sent message back to server message) + **F4**
(resolve server Sent/Drafts folders).

**C3. Identities + signatures + send-later** — **L**
Identities table (0006), per-identity signature/reply-to/SMTP, From-picker in
composer; send-later reuses outbox `next_attempt_unix` + a time-based promoter.
*Depends on:* nothing hard; enables C1 signatures.

### Content / security clusters

**S1. Attachment download / open / save + inline cid images** — **M**
Command to serve attachment bytes (`commands/detail.rs`), fs/dialog capability,
`sanitize_attachment_filename` on write; inline images need un-filtering DTO parts +
an iframe custom-protocol/data-URI resolver (breach-sensitive vs ADR-0007 — design
carefully).
*Depends on:* fs/dialog capability. **Security-reviewable.**

**S2. Remote-content allow (per-sender/message)** — **M**
Parameterize `sanitize_message_html` + `message_policy()` so the img@src arm can keep
http(s); add per-message/per-sender allowlist storage + UI message-bar override.
*Depends on:* nothing.

**S3. PGP / S-MIME** — **XL**
Key handling, sign/verify/encrypt/decrypt, trust UI, MIME typed extraction. No hooks
today.
*Depends on:* nothing structural but very large; likely late.

**S4. iCalendar invitations (iMIP)** — **L**
Typed `text/calendar` extraction, invitation card, accept/decline/RSVP (writes a
reply email).
*Depends on:* **M1** (reply plumbing) for RSVP.

### Efficiency / protocol clusters

**E1. CONDSTORE / QRESYNC incremental sync** — **L**
Add mod-sequence to `MailboxSyncState`/`SyncOutcome` (`highest_modseq` column
reserved), capability model surfaced to the engine, replace the newest-500-UID flag
window + full `SEARCH ALL` present-set with MODSEQ/VANISHED deltas.
*Depends on:* interacts with **F4** (move/expunge break the UID-highwater model —
must land reconciliation before or with folder ops).

**E2. IMAP IDLE / push** — **L**
Persistent-connection job kind, new-mail push event; contends with the
one-connection-per-account model (`detail.rs` already opens a second connection
outside the lock — a latent hazard to fix).
*Depends on:* new-job-kind plumbing.

**E3. OAuth2 / XOAUTH2 (incoming + SMTP)** — **L**
Auth-mechanism enum in `account.rs` (none exists), token source + refresh metadata
storage (preserve no-secrets-in-DB), `Credentials::new_xoauth2` for SMTP, SASL
AUTHENTICATE for IMAP, OAuth token flow at connect. **Required for Gmail / M365** —
axomail cannot connect to them today.
*Depends on:* nothing structural; gates a huge slice of real-world accounts.

### Adjacent-subsystem clusters (net-new)

**A1. Contacts / address book** — **L**
Schema (contacts/cards/lists), collected-addresses store, recipient autocomplete,
optional CardDAV. None exists.
*Depends on:* enables composer autocomplete (improves **C1**).

**A2. Message filters / rules engine** — **L**
Rules storage, condition→action model, run-on-arrival hook in the ingest path +
run-on-demand command, UI CRUD.
*Depends on:* **F2/F3/F4** (actions move/tag/mark-junk need write-back + tags +
folders).

**A3. Junk / spam** — **L**
Per-message junk score column + Bayesian token store, mark-junk/not-junk actions,
per-account training, junk-folder routing.
*Depends on:* **F3** ($Junk keyword) + **F2** (write-back) + **F4** (Junk folder).

**A4. Account-type breadth (NNTP / Local Folders / RSS / EWS / JMAP)** — **XL**
Each is a new `IncomingProtocol` variant + adapter + likely a `RemoteMessageId`
shape + dispatch-enum arm.
*Depends on:* nothing shared; large per protocol.

### UX / platform clusters

**U1. Dark mode + theming** — **S**
Tokens exist; wire `data-theme` + toggle + `prefers-color-scheme`.
*Depends on:* nothing.

**U2. Job/activity manager + cancel UI** — **S/M**
`cancel`/`cancel_all`/`list_by_state` exist unwired — add commands + a panel. Fix the
cancelled-job-reported-as-`Permanent` event ambiguity.
*Depends on:* nothing.

**U3. Account edit/management UI** — **M**
`delete_account` exists but no UI; add edit (keychain-sensitive).
*Depends on:* nothing.

**U4. Global shortcuts / command palette, i18n, print, undo, list virtualization** —
**M–L** each, largely independent. Virtualization pairs naturally with **M4**. i18n
is greenfield (no framework installed). A general "data changed" event (matching the
`SyncEventDto` forwarder pattern) is likely needed once server-side mutations
(F2/F4/A2) can change data outside the current command's view.

### Suggested ordering (dependency-respecting)

1. **F1** (threading identity) — cheap, unblocks M1/M4/C2.
2. **E3** (OAuth2) — gates real accounts; independent.
3. **F2** (write-back + offline queue) — unblocks all durable actions.
4. **F4** (folders) + **E1** (QRESYNC reconciliation) together — folder ops break
   UID-highwater sync, so land reconciliation with them.
5. **F3** (tags) alongside/after F2.
6. Action clusters **M1/M2/M3**, **C2**, then breadth (**A1/A2/A3**, **S1/S2**).
7. UX quick wins (**U1/U2**) can slot in anywhere.
8. **S3** (PGP/S-MIME), **A4** (new protocols) — late, XL.

---

*Sources: parallel subsystem reads of `crates/mail-core`, `mail-imap`, `mail-smtp`,
`mail-pop3`, `mail-sync`, `mail-storage`, `mail-parser`, `mail-security`,
`mail-test-support`, and `apps/desktop` (src-tauri + Vue frontend), plus `docs/adr`.
Line numbers are indicative as of 2026-07-19.*
