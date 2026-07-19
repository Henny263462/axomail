//! Local storage layer.
//!
//! Owns the SQLite schema, versioned SQLx migrations, and the typed query
//! surface (work package 1c). Metadata and state live in SQLite; raw
//! messages and attachment payloads live outside the database. **No
//! credentials are ever stored here** — accounts hold only non-secret
//! [`axomail_core::CredentialRef`] strings pointing into the OS keychain
//! (ADR-0001).
//!
//! # Layout
//!
//! - [`Store`] opens the connection pool, applies the durability
//!   pragmas, and runs migrations.
//! - Typed repositories ([`AccountRepo`], [`MailboxRepo`], [`MessageRepo`],
//!   [`SettingsRepo`]) borrow the pool and translate between
//!   `axomail_core` domain types and rows.
//! - Every `sqlx` failure is mapped into [`axomail_core::MailError`] with
//!   a redacted message that carries only the operation name, never SQL
//!   values (see `error_map`).
#![forbid(unsafe_code)]

mod accounts;
mod address_json;
mod drafts;
mod error_map;
mod identities;
mod identity_prefs;
mod ingest;
mod mailboxes;
mod message_rows;
mod messages;
mod outbox;
mod pagination;
mod pending_ops;
mod pop3_uidl;
mod remote_content;
mod search;
mod settings;
mod store;
mod sync_jobs;
mod sync_state;
mod tags;
mod threading_json;
mod unified;

pub use accounts::{AccountRepo, AccountUpdate, NewAccount};
pub use drafts::{DraftId, DraftRecord, DraftRepo, NewDraft};
pub use identities::{Identity, IdentityFields, IdentityId, IdentityRepo, NewIdentity};
pub use identity_prefs::{ArchiveGranularity, ForwardMode, ReplyPosition, SignaturePlacement};
pub use ingest::{IngestRepo, IngestSummary};
pub use mailboxes::{MailboxRepo, NewMailbox};
pub use messages::{MessageRepo, NewMessage};
pub use outbox::{NewOutbox, OutboxId, OutboxRecord, OutboxRepo, OutboxState};
pub use pagination::{MessageCursor, MessagePage};
pub use pending_ops::{
    NewPendingOp, PendingOp, PendingOpCounts, PendingOpId, PendingOpKind, PendingOpState,
    PendingOpsRepo,
};
pub use pop3_uidl::Pop3UidlRepo;
pub use remote_content::RemoteContentRepo;
pub use search::{SearchHit, SearchRepo};
pub use settings::SettingsRepo;
pub use store::Store;
pub use sync_jobs::{JobKind, JobState, NewSyncJob, SyncJob, SyncJobId, SyncJobRepo};
pub use sync_state::SyncStateRepo;
pub use tags::{KeywordOpPayload, NewTag, Tag, TagColor, TagId, TagRepo, TagUpdate};
pub use unified::{UnifiedInboxPage, UnifiedMessage, UnifiedRepo};
