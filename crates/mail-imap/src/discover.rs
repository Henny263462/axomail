//! Folder discovery via `LIST`, including RFC 6154 special-use roles.

use async_imap::imap_proto::types::NameAttribute;
use async_imap::types::Name;
use axomail_core::{MailError, MailboxName, MailboxRole, RemoteMailbox, RemoteMailboxState};
use futures::TryStreamExt;

use crate::connection::Connected;
use crate::error::{map_imap_error, with_deadline};
use crate::utf7;

/// Lists all selectable mailboxes, decoding modified UTF-7 names and
/// deriving roles from SPECIAL-USE attributes.
pub(crate) async fn discover(
    connected: &mut Connected,
    timeout: std::time::Duration,
) -> Result<RemoteMailboxState, MailError> {
    let names: Vec<Name> = with_deadline(timeout, async {
        let stream = connected
            .session
            .list(Some(""), Some("*"))
            .await
            .map_err(|err| map_imap_error(&err))?;
        stream
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;

    let mailboxes = names
        .iter()
        .filter(|name| !is_unselectable(name))
        .map(|name| RemoteMailbox {
            name: MailboxName::new(utf7::decode(name.name())),
            role: role_from_attributes(name.attributes()),
        })
        .collect();

    Ok(RemoteMailboxState { mailboxes })
}

/// True if the mailbox cannot be selected (`\Noselect`), so it should not
/// appear as a syncable folder.
fn is_unselectable(name: &Name) -> bool {
    name.attributes()
        .iter()
        .any(|attr| matches!(attr, NameAttribute::NoSelect))
}

/// Derives a [`MailboxRole`] hint from SPECIAL-USE attributes (RFC 6154).
///
/// Only server-provided attributes are honored; `None` is returned when
/// the server gives no hint, leaving name-based guessing to the caller
/// (as documented on [`RemoteMailbox::role`]). `INBOX` therefore maps to
/// `None` here, since it carries no SPECIAL-USE attribute.
fn role_from_attributes(attributes: &[NameAttribute<'_>]) -> Option<MailboxRole> {
    attributes.iter().find_map(|attr| match attr {
        NameAttribute::Sent => Some(MailboxRole::Sent),
        NameAttribute::Drafts => Some(MailboxRole::Drafts),
        NameAttribute::Trash => Some(MailboxRole::Trash),
        NameAttribute::Junk => Some(MailboxRole::Junk),
        NameAttribute::Archive | NameAttribute::All => Some(MailboxRole::Archive),
        _ => None,
    })
}
