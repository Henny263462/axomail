//! The POP3 sync arm of the composition root.
//!
//! POP3 is a pure download source with no server-side cursor, so the engine
//! cannot tell the adapter which messages it already has. This wrapper closes
//! that gap: around every [`sync`](IncomingMailProvider::sync) it consults the
//! `pop3_uidl` de-duplication table through [`axomail_storage::Pop3UidlRepo`],
//! feeding the known-`UIDL` set into [`Pop3Provider::set_known_uidls`] so the
//! adapter skips already-downloaded mail.
//!
//! # Crash-safe `UIDL` recording
//!
//! Newly seen `UIDL`s are recorded from *committed* message rows, not from an
//! in-flight [`SyncOutcome`]. [`Pop3UidlRepo::record_ingested`] runs at the
//! start of each pass and copies the `UIDL` of every POP3 message the engine
//! has actually stored so far into the table. The engine ingests each pass
//! before requesting the next, so within a multi-batch session pass N+1 sees
//! pass N's messages and never re-downloads them. Should a crash strike
//! between a pass and its ingest, nothing was committed, so nothing is
//! recorded and the message is simply fetched again — the safe direction.

use axomail_core::{
    AccountId, AppendOutcome, FlagDelta, FlagSet, IncomingMailProvider, MailError, MailboxName,
    RawMessage, RemoteMailboxState, RemoteMessageId, SyncOutcome, SyncRequest,
};
use axomail_pop3::Pop3Provider;
use axomail_storage::Store;

/// A [`Pop3Provider`] wrapped with the storage it needs for the known-`UIDL`
/// flow. Delegates every provider method, adding the de-duplication bookkeeping
/// only around `sync`.
pub struct Pop3Sync {
    provider: Pop3Provider,
    store: Store,
    account: AccountId,
}

impl Pop3Sync {
    /// Wraps `provider` with the `store` and `account` used to resolve and
    /// record known `UIDL`s.
    #[must_use]
    pub fn new(provider: Pop3Provider, store: Store, account: AccountId) -> Self {
        Self {
            provider,
            store,
            account,
        }
    }

    /// Records already-ingested `UIDL`s, then loads the full known set to hand
    /// to the adapter before a pass.
    async fn refresh_known_uidls(&mut self) -> Result<(), MailError> {
        let uidls = self.store.pop3_uidls();
        // Record from committed rows first so the known set includes every
        // message stored by previous passes (crash-safe: see module docs).
        uidls.record_ingested(self.account).await?;
        let known = uidls.known_uidls(self.account).await?;
        self.provider.set_known_uidls(known);
        Ok(())
    }
}

impl IncomingMailProvider for Pop3Sync {
    async fn connect(&mut self) -> Result<(), MailError> {
        self.provider.connect().await
    }

    async fn discover(&mut self) -> Result<RemoteMailboxState, MailError> {
        self.provider.discover().await
    }

    async fn sync(&mut self, request: SyncRequest) -> Result<SyncOutcome, MailError> {
        self.refresh_known_uidls().await?;
        self.provider.sync(request).await
    }

    async fn fetch_message(&mut self, id: &RemoteMessageId) -> Result<RawMessage, MailError> {
        self.provider.fetch_message(id).await
    }

    async fn select_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        self.provider.select_mailbox(mailbox).await
    }

    async fn store_flags(
        &mut self,
        id: &RemoteMessageId,
        delta: &FlagDelta,
    ) -> Result<(), MailError> {
        self.provider.store_flags(id, delta).await
    }

    async fn move_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        self.provider.move_message(id, target).await
    }

    async fn copy_message(
        &mut self,
        id: &RemoteMessageId,
        target: &MailboxName,
    ) -> Result<(), MailError> {
        self.provider.copy_message(id, target).await
    }

    async fn delete_message(&mut self, id: &RemoteMessageId) -> Result<(), MailError> {
        self.provider.delete_message(id).await
    }

    async fn expunge_mailbox(&mut self, mailbox: &MailboxName) -> Result<(), MailError> {
        self.provider.expunge_mailbox(mailbox).await
    }

    async fn append_message(
        &mut self,
        mailbox: &MailboxName,
        flags: &FlagSet,
        raw: &[u8],
    ) -> Result<AppendOutcome, MailError> {
        self.provider.append_message(mailbox, flags, raw).await
    }

    async fn create_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        self.provider.create_mailbox(path).await
    }

    async fn rename_mailbox(
        &mut self,
        from: &MailboxName,
        to: &MailboxName,
    ) -> Result<(), MailError> {
        self.provider.rename_mailbox(from, to).await
    }

    async fn delete_mailbox(&mut self, path: &MailboxName) -> Result<(), MailError> {
        self.provider.delete_mailbox(path).await
    }

    async fn set_mailbox_subscribed(
        &mut self,
        path: &MailboxName,
        subscribed: bool,
    ) -> Result<(), MailError> {
        self.provider.set_mailbox_subscribed(path, subscribed).await
    }

    async fn disconnect(&mut self) -> Result<(), MailError> {
        self.provider.disconnect().await
    }
}
