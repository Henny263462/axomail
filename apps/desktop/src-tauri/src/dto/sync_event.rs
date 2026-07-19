//! Frontend-facing mirror of the sync engine's progress events.

use axomail_core::MailError;
use axomail_sync::{SyncErrorClass, SyncEvent};
use serde::Serialize;

use super::checked_id;

/// A sync progress event forwarded to the frontend as `axomail://sync`.
///
/// This flattens the engine's [`SyncEvent`] tagged union into the single
/// shape the shared contract defines: a `kind` discriminator plus the union
/// of every variant's fields, each absent field serialized as `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEventDto {
    /// Which lifecycle event this is.
    pub kind: &'static str,
    /// Job the event concerns.
    pub job_id: i64,
    /// Mailbox the event concerns, when applicable.
    pub mailbox_id: Option<i64>,
    /// Account the event concerns (write-back `opsReplayed` events).
    pub account_id: Option<i64>,
    /// New messages ingested (batch and finish events).
    pub new_messages: Option<u64>,
    /// Operations replayed to the server (`opsReplayed` events).
    pub replayed: Option<u64>,
    /// Operations parked as failed (`opsReplayed` events).
    pub failed: Option<u64>,
    /// Terminating error class (failure events).
    pub error_class: Option<&'static str>,
    /// Redacted, user-safe message (failure events).
    pub message: Option<String>,
}

impl SyncEventDto {
    /// Maps an engine event to its DTO, validating the ids it carries.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if a job or mailbox id is out of
    /// the JavaScript safe integer range.
    pub fn from_event(event: &SyncEvent) -> Result<Self, MailError> {
        Ok(match event {
            SyncEvent::JobQueued {
                job_id, mailbox_id, ..
            } => {
                let dto = Self::base("jobQueued", job_id.value())?;
                match mailbox_id {
                    Some(mailbox_id) => dto.with_mailbox(mailbox_id.value())?,
                    None => dto,
                }
            }
            SyncEvent::JobStarted { job_id } => Self::base("jobStarted", job_id.value())?,
            SyncEvent::BatchIngested {
                job_id,
                mailbox_id,
                new_messages,
            } => {
                let mut dto = Self::base("batchIngested", job_id.value())?
                    .with_mailbox(mailbox_id.value())?;
                dto.new_messages = Some(*new_messages);
                dto
            }
            SyncEvent::JobFinished { job_id, summary } => {
                let mut dto = Self::base("jobFinished", job_id.value())?;
                dto.new_messages = Some(summary.new_messages);
                dto
            }
            SyncEvent::OpsReplayed {
                job_id,
                account_id,
                replayed,
                failed,
            } => {
                let mut dto = Self::base("opsReplayed", job_id.value())?;
                dto.account_id = Some(checked_id(account_id.value())?);
                dto.replayed = Some(*replayed);
                dto.failed = Some(*failed);
                dto
            }
            SyncEvent::JobCancelled { job_id } => Self::base("jobCancelled", job_id.value())?,
            SyncEvent::JobFailed {
                job_id,
                class,
                message,
            } => {
                let mut dto = Self::base("jobFailed", job_id.value())?;
                dto.error_class = Some(class_str(*class));
                dto.message = Some(message.clone());
                dto
            }
        })
    }

    /// Starts a DTO for `kind` with a validated job id and empty fields.
    fn base(kind: &'static str, job_id: i64) -> Result<Self, MailError> {
        Ok(Self {
            kind,
            job_id: checked_id(job_id)?,
            mailbox_id: None,
            account_id: None,
            new_messages: None,
            replayed: None,
            failed: None,
            error_class: None,
            message: None,
        })
    }

    /// Attaches a validated mailbox id.
    fn with_mailbox(mut self, mailbox_id: i64) -> Result<Self, MailError> {
        self.mailbox_id = Some(checked_id(mailbox_id)?);
        Ok(self)
    }
}

/// Stable camelCase name for an error class value.
fn class_str(class: SyncErrorClass) -> &'static str {
    match class {
        SyncErrorClass::Temporary => "temporary",
        SyncErrorClass::Permanent => "permanent",
        SyncErrorClass::UserConfiguration => "userConfiguration",
    }
}
