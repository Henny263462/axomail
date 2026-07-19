//! DTOs for the reply/forward prefill and source-marking commands.
//!
//! The prefill command returns a [`ComposerPrefillDto`]; the composer echoes
//! its `inReplyTo`/`references`/`attachments` back through `ComposeInput` on
//! send, and — after a successful send — marks the source message via
//! `mark_reply_source` using the returned [`PrefillSourceDto`].

use axomail_core::MailError;
use serde::{Deserialize, Serialize};

use super::checked_id;
use super::compose::ComposeAttachmentDto;

/// Which reply operation a prefill is being prepared for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplyMode {
    /// Reply to the sender only.
    Reply,
    /// Reply to the sender and every other recipient.
    ReplyAll,
    /// Forward to new recipients.
    Forward,
}

/// How a source message is marked once its reply/forward is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceMark {
    /// Set `\Answered` (a reply was sent).
    Answered,
    /// Add the `$Forwarded` keyword (a forward was sent).
    Forwarded,
}

/// The source message a reply/forward came from, and how to mark it on success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefillSourceDto {
    /// Local id of the message being replied to or forwarded.
    pub message_id: i64,
    /// Which flag/keyword to set once the composed message is sent.
    pub mark: SourceMark,
}

/// Everything the composer needs to open pre-filled for a reply or forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerPrefillDto {
    /// Account the reply/forward is composed from.
    pub account_id: i64,
    /// Best-guess sender identity (matched on the delivered-to address), or
    /// `None` when no identity could be matched (the composer picks a default).
    pub identity_id: Option<i64>,
    /// Prefilled primary recipients.
    pub to: Vec<String>,
    /// Prefilled carbon-copy recipients.
    pub cc: Vec<String>,
    /// Normalized subject (`Re:`/`Fwd:`).
    pub subject: String,
    /// Prefilled body: the citation/forward block, signature, and reply area,
    /// arranged per the identity's composition preferences.
    pub body: String,
    /// `In-Reply-To` header value for a reply (source Message-ID, no brackets).
    pub in_reply_to: Option<String>,
    /// `References` chain for a reply (oldest first).
    pub references: Vec<String>,
    /// Attachments to carry (a forwarded message as `message/rfc822`).
    pub attachments: Vec<ComposeAttachmentDto>,
    /// The source message and how to mark it after a successful send.
    pub source: PrefillSourceDto,
}

impl ComposerPrefillDto {
    /// Validates the ids that cross to JavaScript.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] if the account, identity, or source
    /// message id is outside the safe integer range.
    pub fn checked(mut self) -> Result<Self, MailError> {
        self.account_id = checked_id(self.account_id)?;
        self.identity_id = self.identity_id.map(checked_id).transpose()?;
        self.source.message_id = checked_id(self.source.message_id)?;
        Ok(self)
    }
}
