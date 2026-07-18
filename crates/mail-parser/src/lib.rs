//! MIME parsing layer.
//!
//! Parses raw RFC 5322/MIME messages into the internal message model
//! (phase 2), tolerating common real-world malformations. Package is named
//! `axomail-parser` to avoid clashing with the public `mail-parser` crate.
#![forbid(unsafe_code)]
