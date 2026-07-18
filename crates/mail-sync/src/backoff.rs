//! Exponential backoff with jitter for transient-failure retries.
//!
//! Delay for failure `n` (1-based) is `base * 2^(n-1)` capped at
//! `backoff_cap`, then softened with "equal jitter" (half the capped
//! delay plus a random amount up to the other half) to avoid retry
//! stampedes across accounts. The jitter source is a small process-wide
//! xorshift generator: it needs no external crate and is not
//! security-sensitive.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::SyncConfig;

/// Computes the backoff delay before retrying after transient failure
/// number `attempt` (1-based).
pub(crate) fn backoff_delay(attempt: u32, config: &SyncConfig) -> Duration {
    let capped = capped_delay(attempt, config);
    let capped_ms = u64::try_from(capped.as_millis()).unwrap_or(u64::MAX);
    let half = capped_ms / 2;
    let span = capped_ms - half;
    let jitter = if span == 0 {
        0
    } else {
        next_rand() % (span + 1)
    };
    Duration::from_millis(half + jitter)
}

/// The exponential delay for `attempt`, clamped to the configured cap.
fn capped_delay(attempt: u32, config: &SyncConfig) -> Duration {
    let shift = attempt.saturating_sub(1).min(31);
    let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let base_ms = u64::try_from(config.backoff_base.as_millis()).unwrap_or(u64::MAX);
    let scaled_ms = base_ms.saturating_mul(factor);
    let scaled = Duration::from_millis(scaled_ms);
    scaled.min(config.backoff_cap)
}

/// Draws the next pseudo-random `u64` from a lazily seeded xorshift state.
fn next_rand() -> u64 {
    static STATE: AtomicU64 = AtomicU64::new(0);

    let mut current = STATE.load(Ordering::Relaxed);
    loop {
        let seed = if current == 0 { seed_value() } else { current };
        let mut x = seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        match STATE.compare_exchange_weak(current, x, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return x,
            Err(observed) => current = observed,
        }
    }
}

/// A non-zero seed derived from the wall clock.
fn seed_value() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or(0)) | 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SyncConfig {
        SyncConfig {
            backoff_base: Duration::from_secs(2),
            backoff_cap: Duration::from_secs(300),
            ..SyncConfig::default()
        }
    }

    #[test]
    fn grows_exponentially_within_bounds() {
        let config = config();
        // Equal jitter keeps every delay in [capped/2, capped].
        for attempt in 1..=4 {
            let capped = capped_delay(attempt, &config);
            let delay = backoff_delay(attempt, &config);
            assert!(delay >= capped / 2, "attempt {attempt}: {delay:?} < half");
            assert!(delay <= capped, "attempt {attempt}: {delay:?} > capped");
        }
    }

    #[test]
    fn respects_the_cap() {
        let config = config();
        // A very high attempt saturates at the cap, never overflowing.
        let capped = capped_delay(30, &config);
        assert_eq!(capped, config.backoff_cap);
        let delay = backoff_delay(30, &config);
        assert!(delay <= config.backoff_cap);
    }
}
