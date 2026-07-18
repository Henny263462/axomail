//! Local storage layer.
//!
//! Owns the SQLite schema, migrations, and query surface (work package 1c),
//! plus the on-disk attachment store with atomic writes and integrity
//! hashes. No credentials are ever stored here.
#![forbid(unsafe_code)]
