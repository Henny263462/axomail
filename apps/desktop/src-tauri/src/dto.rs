//! Data-transfer objects exchanged with the Vue frontend.
//!
//! Every field name matches the shared command contract; serialization uses
//! `camelCase`. List and detail DTOs carry only what their views render;
//! passwords and credential references never cross this boundary, and the
//! message-detail body is sanitized before it is placed in a DTO.
//!
//! # ID precision invariant
//!
//! Rust row identifiers are `i64` while JavaScript numbers are exact only up
//! to `2^53 - 1`. Every identifier that crosses the IPC boundary is checked
//! by [`checked_id`]; an out-of-range id yields a [`MailError::Configuration`]
//! instead of silently losing precision in the frontend (see the matching
//! note in `apps/desktop/src/types/mail.ts`). New identifiers introduced by
//! the composer, detail, and sync surfaces are guarded the same way.

use axomail_core::MailError;

mod account;
mod actions;
mod attachments;
mod autoconfig;
mod compose;
mod connection;
mod contact;
mod detail;
mod filter;
mod folder;
mod identity;
mod invitation;
mod jobs;
mod junk;
mod mbox;
mod message;
mod oauth;
mod offline;
mod push_event;
mod reply;
mod search;
mod settings;
mod sync_event;
mod tag;
mod unified;

pub use account::{
    AccountDetailDto, AccountSummaryDto, AuthInput, CreateAccountInput, IncomingInput,
    IncomingServerDetailDto, MailboxSummaryDto, OAuthInput, OutgoingServerDetailDto,
    ServerEndpointInput, ServerInput, UpdateAccountInput,
};
pub use actions::{CompactOutcomeDto, MessageMoveDto, MoveOutcomeDto};
pub use attachments::SavedAttachmentDto;
pub use autoconfig::{AutoconfigIncomingDto, AutoconfigOutgoingDto, AutoconfigResultDto};
pub use compose::{
    AttachmentCandidateDto, ComposeAttachmentDto, ComposeInput, DraftDto, OutboxItemDto,
    SendOutcomeDto,
};
pub use connection::ConnectionTestResult;
pub use contact::{
    ContactDetailDto, ContactDto, ContactEmailDto, ContactEmailInput, ContactGroupDto,
    ContactInput, RecipientSuggestionDto, VCardImportDto,
};
pub use detail::{AttachmentMetaDto, MessageDetailDto, RecipientDto, ScamFlagDto};
pub use filter::{
    AccountFilterSettingsDto, FilterActionDto, FilterConditionDto, FilterDto, FilterInputDto,
    FilterRunOutcomeDto,
};
pub use folder::{CreateFolderInput, RenameFolderInput, SetFolderSubscribedInput};
pub use identity::{IdentityDto, IdentityInput};
pub use invitation::{InvitationDto, InvitationResponseDto, RsvpChoice};
pub use jobs::SyncJobDto;
pub use junk::JunkSettingsDto;
pub use mbox::{MboxExportDto, MboxImportDto};
pub use message::{MessageListItemDto, MessageListPageDto};
pub use oauth::{OAuthAuthorizeResult, OAuthAuthorizeStatus};
pub use offline::OfflinePolicyDto;
pub use push_event::PushStatusDto;
pub use reply::{ComposerPrefillDto, PrefillSourceDto, ReplyMode, SourceMark};
pub use search::SearchResultDto;
pub use settings::{LanguagePreference, Pop3DeletePolicyDto, SettingsDto, ThemePreference};
pub use sync_event::SyncEventDto;
pub use tag::{MessageTagOptionDto, MessageTagOptionsDto, TagChipDto, TagDto};
pub use unified::{UnifiedItemDto, UnifiedPageDto};

/// Largest integer a JavaScript `number` can represent exactly: `2^53 - 1`.
pub(crate) const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Validates that a database identifier survives the crossing to JavaScript.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if `value` is negative or exceeds
/// [`MAX_SAFE_INTEGER`], the two cases in which a JavaScript `number` could
/// not represent it exactly.
pub fn checked_id(value: i64) -> Result<i64, MailError> {
    if (0..=MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(MailError::Configuration {
            message: "identifier exceeds the safe integer range".to_owned(),
        })
    }
}
