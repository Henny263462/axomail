//! IMAP protocol adapter.
//!
//! Implements the incoming-provider traits from `axomail-core` for IMAP
//! (phase 2): UID-based two-way synchronization, capability detection,
//! mailbox listing, and flag sync. IMAP-specific types must not leak out
//! of this crate.
#![forbid(unsafe_code)]
