//! SMTP protocol adapter.
//!
//! Implements the outgoing-provider trait from `axomail-core` (phase 2):
//! authenticated submission over TLS/STARTTLS, correct MIME assembly
//! delegation, and duplicate-send protection.
#![forbid(unsafe_code)]
