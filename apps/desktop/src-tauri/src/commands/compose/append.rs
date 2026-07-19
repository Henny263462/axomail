//! Best-effort upload of the Sent copy to the server via IMAP `APPEND`.
//!
//! After SMTP accepts a message, the same rendered bytes are filed to the
//! account's Sent folder on the server so the sent mail is visible from every
//! device. This is a **post-send** step: the mail is already delivered, so a
//! failure here never fails the send — it is logged, and the local Sent copy
//! remains. The upload runs on a background task ([`spawn_sent_copy_append`]) so
//! a slow or unreachable IMAP server does not block the composer.
//!
//! Double-filing is avoided by adopting the server copy: [`append_message`]
//! resolves the appended message's UID (matching by `Message-ID`), and
//! [`adopt_remote_id`] records it on the local Sent row, so the next sync of the
//! Sent folder recognizes the message instead of inserting a duplicate.
//!
//! [`append_message`]: axomail_core::IncomingMailProvider::append_message
//! [`adopt_remote_id`]: axomail_storage::MessageRepo::adopt_remote_id

use axomail_core::{AccountId, FlagSet, IncomingMailProvider, MailError, MailboxName, MessageId};
use axomail_storage::Store;
use axomail_sync::ProviderFactory;

use crate::composition::AppProviderFactory;

/// Everything the background step needs to upload one Sent copy and adopt the
/// server identity back onto the local record.
#[derive(Debug)]
pub struct SentCopyPlan {
    /// Account whose IMAP connection files the copy.
    pub account_id: AccountId,
    /// The local Sent-copy row to adopt the resolved server identity onto.
    pub local_copy_id: MessageId,
    /// Full server path of the target (Sent) folder.
    pub mailbox_remote: MailboxName,
    /// Flags to tag the appended message with (`\Seen` for a Sent copy).
    pub flags: FlagSet,
    /// The exact RFC 5322 bytes SMTP delivered.
    pub raw: Vec<u8>,
}

/// Uploads the Sent copy for `plan` and adopts the resolved server identity onto
/// the local row. Generic over the provider factory so tests can drive it with a
/// scripted server or a fake.
///
/// Best-effort: an account that no longer exists is a silent no-op, and a POP3
/// account's `UnsupportedCapability` (or any connect/append failure) propagates
/// as an error the caller logs and swallows.
///
/// # Errors
///
/// Returns whatever error the connect, append, or adopt step raised.
pub async fn append_sent_copy_impl<P, F>(
    store: &Store,
    factory: &F,
    plan: &SentCopyPlan,
) -> Result<(), MailError>
where
    P: IncomingMailProvider,
    F: ProviderFactory<P>,
{
    let Some(account) = store.accounts().get(plan.account_id).await? else {
        return Ok(());
    };
    let mut provider = factory.make(&account).await?;
    provider.connect().await?;
    let result = run_append(store, &mut provider, plan).await;
    // Close regardless of the append outcome; a teardown error is not reported.
    let _ = provider.disconnect().await;
    result
}

/// Appends the copy and, if the server identity was resolved, adopts it.
async fn run_append<P: IncomingMailProvider>(
    store: &Store,
    provider: &mut P,
    plan: &SentCopyPlan,
) -> Result<(), MailError> {
    let outcome = provider
        .append_message(&plan.mailbox_remote, &plan.flags, &plan.raw)
        .await?;
    if let Some(remote_id) = outcome.remote_id {
        store
            .messages()
            .adopt_remote_id(plan.local_copy_id, &remote_id)
            .await?;
    }
    Ok(())
}

/// Spawns [`append_sent_copy_impl`] as a detached background task so the send
/// command returns immediately. Failures are logged (the message is already
/// delivered and the local copy is filed), never surfaced.
pub(crate) fn spawn_sent_copy_append(
    store: &Store,
    factory: &AppProviderFactory,
    plan: SentCopyPlan,
) {
    let store = store.clone();
    let factory = factory.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = append_sent_copy_impl(&store, &factory, &plan).await {
            tracing::warn!(error = %error, "could not upload the Sent copy to the server");
        }
    });
}
