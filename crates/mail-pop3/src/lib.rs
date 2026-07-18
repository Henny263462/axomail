//! POP3 protocol adapter.
//!
//! Implements the incoming-provider traits from `axomail-core` for POP3
//! (phase 3). POP3 is treated strictly as a download source: UIDL-based
//! dedup, no server-side deletion without an explicit opt-in policy.
#![forbid(unsafe_code)]
