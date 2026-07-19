//! Tuning parameters for the [`crate::SyncEngine`].

use std::time::Duration;

/// Configuration for the synchronization engine.
///
/// [`SyncConfig::default`] provides production-sensible values; tests and
/// the app layer override individual fields as needed.
#[derive(Debug, Clone, Copy)]
pub struct SyncConfig {
    /// Maximum number of jobs executing at once across all accounts.
    ///
    /// Independent of the per-account rule that never lets one account
    /// run two jobs concurrently.
    pub max_parallel_jobs: usize,
    /// Deadline applied to every individual network operation
    /// (connect, each sync pass, disconnect); exceeding it maps to
    /// [`axomail_core::MailError::Timeout`].
    pub network_timeout: Duration,
    /// Maximum envelopes requested per sync pass.
    pub batch_size: u32,
    /// Maximum raw message bodies downloaded for offline availability in a
    /// single mailbox-sync job. A larger backlog progresses over subsequent
    /// passes, keeping any one job bounded.
    pub offline_download_batch: u32,
    /// Maximum transient failures tolerated within a single job before it
    /// is failed.
    pub max_attempts: u32,
    /// Cadence of the QRESYNC present-set safety net: every Nth mailbox-sync
    /// job (per mailbox, counted in-process) recomputes removals from the full
    /// authoritative present set instead of trusting the VANISHED delta alone,
    /// backstopping any silently missed expunge. The first job after start is
    /// always a reconcile (an empty counter reconciles), so a restart also
    /// re-verifies. `1` reconciles every pass; only the CONDSTORE+QRESYNC fast
    /// path consults it.
    pub full_reconcile_every: u32,
    /// Base delay for the first backoff step.
    pub backoff_base: Duration,
    /// Upper bound on any single backoff delay.
    pub backoff_cap: Duration,
    /// Capacity of the bounded enqueue channel (backpressure bound).
    pub command_buffer: usize,
    /// Capacity of the broadcast event channel.
    pub event_buffer: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_parallel_jobs: 2,
            network_timeout: Duration::from_secs(30),
            batch_size: 200,
            offline_download_batch: 25,
            max_attempts: 4,
            full_reconcile_every: 10,
            backoff_base: Duration::from_secs(2),
            backoff_cap: Duration::from_secs(300),
            command_buffer: 256,
            event_buffer: 256,
        }
    }
}
