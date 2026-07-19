//! MIME parsing layer.
//!
//! Parses raw RFC 5322/MIME messages into [`ParsedMessage`], the internal
//! message model used by storage, sync, and the UI. Parsing is deliberately
//! tolerant of real-world malformations: the underlying `mail-parser` crate
//! recovers from most defects, so only input it cannot interpret at all maps
//! to [`axomail_core::MailError::Parse`].
//!
//! This layer performs no HTML sanitization. The extracted `html_body` is the
//! decoded-but-untrusted markup; safe rendering is the exclusive job of
//! `axomail-security` (ADR-0007). The `preview` field is a plain-text summary
//! for list views only, produced with a tolerant tag stripper.
//!
//! The package is named `axomail-parser` to avoid clashing with the public
//! `mail-parser` crate it wraps (ADR-0002).
#![forbid(unsafe_code)]

mod address;
mod model;
mod parse;
mod preview;
mod redact;
mod text;

pub use model::{ParsedAttachment, ParsedMessage};
pub use parse::parse_message;
pub use text::html_to_plain_text;
