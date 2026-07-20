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
mod calendar_invites;
mod contacts;
mod drafts;
mod error_map;
mod filters;
mod identities;
mod identity_prefs;
mod ingest;
mod junk;
mod mailboxes;
mod message_disposal;
mod message_rows;
mod messages;
mod offline;
mod outbox;
mod pagination;
mod pending_ops;
mod pgp_keys;
mod pop3_uidl;
mod raw_store;
mod remote_content;
mod retention;
mod saved_searches;
mod search;
mod settings;
mod store;
mod sync_jobs;
mod sync_state;
mod tags;
mod threading_json;
mod unified;
mod vcard;

pub use accounts::{AccountRepo, AccountUpdate, NewAccount};
pub use calendar_invites::{CalendarInvite, CalendarInviteRepo, InviteId, NewInvite, RsvpStatus};
pub use contacts::{
    Contact, ContactEmail, ContactGroup, ContactGroupId, ContactId, ContactMatch, ContactRepo,
    ContactUpdate, NewContact, NewContactEmail,
};
pub use drafts::{DraftId, DraftRecord, DraftRepo, NewDraft};
pub use filters::{
    ActionKind, ArrivalState, ConditionField, ConditionOp, Filter, FilterAction, FilterCondition,
    FilterId, FilterInput, FilterMessageFacts, FilterProgram, FilterRepo, MatchType,
    validate_filter_input,
};
pub use identities::{Identity, IdentityFields, IdentityId, IdentityRepo, NewIdentity};
pub use identity_prefs::{ArchiveGranularity, ForwardMode, ReplyPosition, SignaturePlacement};
pub use ingest::{IngestRepo, IngestSummary};
pub use junk::{JunkRepo, JunkSettings};
pub use mailboxes::{DiscoveredMailbox, MailboxCounts, MailboxRepo, NewMailbox, ReconcileSummary};
pub use messages::{MessageRepo, NewMessage};
pub use offline::{DownloadCandidate, DownloadPlan, OfflineMode, OfflinePolicy, OfflinePolicyRepo};
pub use outbox::{NewOutbox, OutboxId, OutboxRecord, OutboxRepo, OutboxState};
pub use pagination::{MessageCursor, MessagePage};
pub use pending_ops::{
    NewPendingOp, PendingOp, PendingOpCounts, PendingOpId, PendingOpKind, PendingOpState,
    PendingOpsRepo,
};
pub use pgp_keys::{
    NewPgpOwnKey, NewPgpRecipientKey, PgpKeyRepo, PgpOwnKey, PgpOwnKeyId, PgpRecipientKey,
    PgpRecipientKeyId,
};
pub use pop3_uidl::Pop3UidlRepo;
pub use raw_store::{raw_message_path, read_raw, remove_raw, write_raw_atomically};
pub use remote_content::RemoteContentRepo;
pub use retention::{RetentionAction, RetentionMode, RetentionPolicy, RetentionRepo};
pub use saved_searches::{
    SavedSearch, SavedSearchId, SavedSearchInput, SavedSearchRepo, SearchCondition, SearchField,
    SearchMatch, SearchQuery, SearchScope, SearchSort, validate_saved_search_input,
    validate_search_query,
};
pub use search::{SearchHit, SearchRepo};
pub use settings::SettingsRepo;
pub use store::Store;
pub use sync_jobs::{JobKind, JobState, NewSyncJob, SyncJob, SyncJobId, SyncJobRepo};
pub use sync_state::SyncStateRepo;
pub use tags::{NewTag, Tag, TagColor, TagId, TagRepo, TagUpdate};
pub use unified::{UnifiedInboxPage, UnifiedMessage, UnifiedRepo};
pub use vcard::{VCard, VCardEmail, VCardVersion, emit_vcards, parse_vcards};
