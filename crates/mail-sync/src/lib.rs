//! Synchronization engine.
//!
//! Protocol-agnostic job orchestration (phase 2+): cancellable jobs with
//! persistent state, bounded parallelism, backoff, and incremental
//! progress events for the frontend.
//!
//! # Design
//!
//! The engine is generic over an [`IncomingMailProvider`](axomail_core::IncomingMailProvider)
//! and a [`ProviderFactory`] that builds one for an account. Concrete
//! protocol crates therefore stay out of this crate; composition (and
//! ADR-0004's dispatch enum) belongs to the app layer.
//!
//! Work is described by [`SyncMailbox`] and enqueued over a bounded
//! channel. A single dispatcher persists each job in the `sync_jobs`
//! table, de-duplicates equivalent pending/running work, and starts a
//! worker. Workers run under two guarantees: a global [`SyncConfig`]
//! parallelism bound, and a per-account lock ensuring one account never
//! runs two jobs at once. Each job connects, then loops sync passes while
//! the server reports more, persisting every pass through
//! [`axomail_storage::IngestRepo`] so progress survives a crash.
//!
//! Transient failures ([`ErrorClass::Temporary`](axomail_core::ErrorClass))
//! retry with exponential backoff and jitter up to a bounded budget;
//! permanent, configuration, and cancellation errors fail the job with
//! their class recorded. Every network operation is wrapped in a
//! configurable timeout. Progress is reported as [`SyncEvent`]s over a
//! broadcast channel. On startup the engine requeues jobs a crash left
//! `running` and resumes everything still `pending`.
#![forbid(unsafe_code)]

mod backoff;
mod config;
mod engine;
mod event;
mod factory;
mod job;
mod op_payload;
mod scheduler;
mod shared;
mod worker;

pub use config::SyncConfig;
pub use engine::{EngineOptions, SyncEngine};
pub use event::{SyncErrorClass, SyncEvent, SyncJobSummary};
pub use factory::ProviderFactory;
pub use job::{SyncMailbox, WorkRequest};
// Write-back op-payload builders, used by the command layer to enqueue ops.
pub use op_payload::{answered_op, delete_op, flagged_op, keyword_op, move_op, seen_op};
pub use scheduler::{BackgroundScheduler, SchedulerConfig, SchedulerStatus};

// Re-exported so callers can name job identifiers and states without a
// direct dependency on the storage crate's job vocabulary.
pub use axomail_storage::{JobState, SyncJobId};
