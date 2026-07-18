# 0004. Native async traits, enum dispatch, no `async-trait` crate

Date: 2026-07-18
Status: accepted

## Context

The provider traits (`IncomingMailProvider`, `OutgoingMailProvider`)
are async. Rust supports native `async fn` in traits since 1.75, but
such traits are not object-safe, and plain `async fn` signatures cannot
demand `Send` futures from implementors. The alternatives are the
`async-trait` crate (boxing, object-safe) or static dispatch.

## Decision

- The traits use **return-position `impl Future + Send`** (the
  desugared form of `async fn`), which lets the trait definition demand
  `Send` futures — required because the sync engine drives providers
  from Tokio worker tasks. Implementors still write plain `async fn`;
  a compile-time test in `provider.rs` proves this works.
- **No `dyn` dispatch.** With exactly three protocol adapters planned,
  `mail-sync` will wrap concrete adapters in an enum
  (`enum AnyIncomingProvider { Imap(...), Pop3(...) }`) and match on
  it. This avoids boxing every future, keeps the `async-trait` crate
  and its per-call allocations out of the tree, and keeps trait
  signatures free of lifetime gymnastics.
- Revisit only if a plugin system ever requires open-ended provider
  sets; that would be its own ADR (JMAP is the earliest candidate).

## Consequences

- Adding a protocol means adding an enum arm in `mail-sync` — explicit
  and cheap at this scale.
- Trait objects (`Box<dyn IncomingMailProvider>`) are intentionally
  impossible; code that wants heterogeneity must go through the enum.
- The `Send` bound is part of the public contract now; a hypothetical
  single-threaded runtime would need parallel `Local` traits, which we
  accept as a non-goal.
