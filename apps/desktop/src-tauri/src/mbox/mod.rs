//! mbox container format: streaming serialization and parsing.
//!
//! This module is the pure, IO-agnostic core behind the folder export/import
//! commands ([`crate::commands::mbox`]). It knows only how to frame and unframe
//! RFC 5322 messages in the Unix mbox format; sourcing raw bytes, the offline
//! gate, and storage ingestion all live in the command layer.
//!
//! - [`write_message`] appends one message to a writer as an **mboxrd** record
//!   (`From ` postmark + `>From`-escaped body + blank-line separator), so a
//!   whole folder can be streamed to disk one message at a time.
//! - [`MboxReader`] pulls messages back out of a buffered reader one at a time,
//!   un-escaping `>From` lines and honouring `Content-Length` when it genuinely
//!   applies — the inverse of the writer, and tolerant of files produced by
//!   other clients.

mod read;
mod write;

pub use read::MboxReader;
pub use write::write_message;
