//! The [`Store`]: connection pool ownership, durability configuration,
//! and migration application.
//!
//! ## Journaling: WAL
//!
//! File-backed stores run in write-ahead-logging mode
//! (`journal_mode = WAL`) with `synchronous = NORMAL`. WAL lets readers
//! (list views, search) proceed concurrently with the sync writer, which
//! matters for a responsive UI, and `NORMAL` keeps the write path fast
//! while remaining crash-safe in WAL mode (a power loss can lose only the
//! last, not-yet-checkpointed transaction, never the database). All three
//! desktop targets — Windows, macOS, Linux — support WAL on local
//! filesystems, which is the only place the app-data database lives.
//! In-memory stores keep SQLite's default journaling, since WAL is not
//! meaningful there.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use axomail_core::MailError;
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::accounts::AccountRepo;
use crate::drafts::DraftRepo;
use crate::error_map::{map_migrate, map_sqlx};
use crate::identities::IdentityRepo;
use crate::ingest::IngestRepo;
use crate::mailboxes::MailboxRepo;
use crate::messages::MessageRepo;
use crate::offline::OfflinePolicyRepo;
use crate::outbox::OutboxRepo;
use crate::pending_ops::PendingOpsRepo;
use crate::pop3_uidl::Pop3UidlRepo;
use crate::remote_content::RemoteContentRepo;
use crate::search::SearchRepo;
use crate::settings::SettingsRepo;
use crate::sync_jobs::SyncJobRepo;
use crate::sync_state::SyncStateRepo;
use crate::tags::TagRepo;
use crate::unified::UnifiedRepo;

/// Migrations embedded at compile time from `migrations/` (ADR-0001).
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// How long a connection waits on a locked database before failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle to the local SQLite database.
///
/// Cloning is cheap: the underlying pool is reference-counted, so clones
/// share the same connections and can be moved onto worker tasks.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens (creating if necessary) a file-backed store at `path`,
    /// applies the durability pragmas, and runs pending migrations.
    ///
    /// Parent directories are created as needed.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Filesystem`] if the parent directory cannot
    /// be created, or [`MailError::Database`] if the connection or
    /// migrations fail.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, MailError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| MailError::Filesystem {
                    message: format!("creating database directory: {}", error.kind()),
                })?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT);

        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .map_err(|error| map_sqlx("open_database", error))?;

        Self::from_pool(pool).await
    }

    /// Opens a private in-memory store for tests and ephemeral use.
    ///
    /// The pool is pinned to a single connection so the in-memory
    /// database persists for the lifetime of the [`Store`].
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Database`] if the connection or migrations
    /// fail.
    pub async fn open_in_memory() -> Result<Self, MailError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(|error| map_sqlx("open_in_memory", error))?
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|error| map_sqlx("open_in_memory", error))?;

        Self::from_pool(pool).await
    }

    /// Runs migrations against a freshly opened pool and wraps it.
    async fn from_pool(pool: SqlitePool) -> Result<Self, MailError> {
        MIGRATOR.run(&pool).await.map_err(map_migrate)?;
        Ok(Self { pool })
    }

    /// The account repository.
    #[must_use]
    pub fn accounts(&self) -> AccountRepo<'_> {
        AccountRepo::new(&self.pool)
    }

    /// The mailbox repository.
    #[must_use]
    pub fn mailboxes(&self) -> MailboxRepo<'_> {
        MailboxRepo::new(&self.pool)
    }

    /// The message repository.
    #[must_use]
    pub fn messages(&self) -> MessageRepo<'_> {
        MessageRepo::new(&self.pool)
    }

    /// The key/value settings repository.
    #[must_use]
    pub fn settings(&self) -> SettingsRepo<'_> {
        SettingsRepo::new(&self.pool)
    }

    /// The per-mailbox IMAP sync-state repository.
    #[must_use]
    pub fn sync_state(&self) -> SyncStateRepo<'_> {
        SyncStateRepo::new(&self.pool)
    }

    /// The background-job repository.
    #[must_use]
    pub fn sync_jobs(&self) -> SyncJobRepo<'_> {
        SyncJobRepo::new(&self.pool)
    }

    /// The sync-outcome ingestion repository.
    #[must_use]
    pub fn ingest(&self) -> IngestRepo<'_> {
        IngestRepo::new(&self.pool)
    }

    /// The composer draft repository.
    #[must_use]
    pub fn drafts(&self) -> DraftRepo<'_> {
        DraftRepo::new(&self.pool)
    }

    /// The per-account sender-identity repository.
    #[must_use]
    pub fn identities(&self) -> IdentityRepo<'_> {
        IdentityRepo::new(&self.pool)
    }

    /// The outgoing send-queue (outbox) repository.
    #[must_use]
    pub fn outbox(&self) -> OutboxRepo<'_> {
        OutboxRepo::new(&self.pool)
    }

    /// The per-mailbox offline-policy and raw-body bookkeeping repository.
    #[must_use]
    pub fn offline(&self) -> OfflinePolicyRepo<'_> {
        OfflinePolicyRepo::new(&self.pool)
    }

    /// The offline operation-queue (write-back) repository.
    #[must_use]
    pub fn pending_ops(&self) -> PendingOpsRepo<'_> {
        PendingOpsRepo::new(&self.pool)
    }

    /// The tag / IMAP-keyword repository.
    #[must_use]
    pub fn tags(&self) -> TagRepo<'_> {
        TagRepo::new(&self.pool)
    }

    /// The POP3 `UIDL` de-duplication repository.
    #[must_use]
    pub fn pop3_uidls(&self) -> Pop3UidlRepo<'_> {
        Pop3UidlRepo::new(&self.pool)
    }

    /// The full-text search repository.
    #[must_use]
    pub fn search(&self) -> SearchRepo<'_> {
        SearchRepo::new(&self.pool)
    }

    /// The per-sender remote-content allow-list repository.
    #[must_use]
    pub fn remote_content(&self) -> RemoteContentRepo<'_> {
        RemoteContentRepo::new(&self.pool)
    }

    /// The cross-account unified-inbox repository.
    #[must_use]
    pub fn unified(&self) -> UnifiedRepo<'_> {
        UnifiedRepo::new(&self.pool)
    }
}
