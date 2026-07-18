//! Synchronization engine.
//!
//! Protocol-agnostic job orchestration (phase 2+): cancellable jobs with
//! persistent state, bounded parallelism, backoff, offline detection, and
//! incremental progress events for the frontend.
#![forbid(unsafe_code)]
