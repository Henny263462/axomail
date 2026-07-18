//! One synchronization pass over a POP3 mailbox.
//!
//! POP3 has no `UIDVALIDITY` and no server-side folders: the single mailbox
//! is identified purely by each message's `UIDL`. A pass lists the mailbox,
//! establishes a stable identity per message, and reports which messages the
//! engine has not seen before (as [`RemoteEnvelope`]s) and which known ones
//! have disappeared from the server.
//!
//! Because [`SyncRequest`] carries no set of already-known `UIDL`s, the
//! caller supplies that set separately; see
//! [`Pop3Provider::set_known_uidls`](crate::Pop3Provider::set_known_uidls).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use axomail_core::{
    Address, MailError, MessageFlags, RemoteEnvelope, RemoteMessageId, SyncOutcome, SyncRequest,
    UtcTimestamp,
};

use crate::command;
use crate::connection::Connected;
use crate::headers::{self, ScannedHeaders};

/// Maps each message's `UIDL` to its session-scoped message number.
///
/// POP3 message numbers are only valid within the connection that issued
/// them, so this map is rebuilt on every sync and used by per-message
/// operations (`RETR`, `DELE`) during the same session.
pub(crate) struct SessionMap {
    by_uidl: HashMap<String, u32>,
}

impl SessionMap {
    /// Returns the current message number for a `UIDL`, if present.
    pub(crate) fn number_for(&self, uidl: &str) -> Option<u32> {
        self.by_uidl.get(uidl).copied()
    }
}

/// One message's identity within this session.
struct Identified {
    number: u32,
    uidl: String,
    size: u64,
    /// Header bytes already fetched (digest mode), reused for the envelope.
    headers: Option<Vec<u8>>,
}

/// Runs a synchronization pass, returning the outcome to persist and the
/// session number map for subsequent per-message operations.
pub(crate) async fn run(
    connected: &mut Connected,
    request: &SyncRequest,
    known: &HashSet<String>,
    timeout: Duration,
) -> Result<(SyncOutcome, SessionMap), MailError> {
    tracing::debug!(mailbox = %request.remote_name, "starting POP3 sync pass");
    // Capabilities are read before the wire is borrowed mutably.
    let top_from_caps = if connected.caps_known() {
        connected.has_top()
    } else {
        true
    };
    let wire = connected.wire();

    let (count, _total) = command::stat(wire, timeout).await?;
    if count == 0 {
        return Ok((
            empty_outcome(known),
            SessionMap {
                by_uidl: HashMap::new(),
            },
        ));
    }

    let list = command::list_all(wire, timeout).await?;
    let uidl = command::uidl_all(wire, timeout).await?;

    let mut top_available = top_from_caps;
    let identified = match uidl {
        Some(pairs) => by_uidl(&list, pairs),
        None => by_digest(wire, &list, &mut top_available, timeout).await?,
    };

    let present: HashSet<&str> = identified.iter().map(|m| m.uidl.as_str()).collect();
    let removed = known
        .iter()
        .filter(|uidl| !present.contains(uidl.as_str()))
        .map(|uidl| RemoteMessageId::Pop3 { uidl: uidl.clone() })
        .collect();

    let by_uidl = identified
        .iter()
        .map(|m| (m.uidl.clone(), m.number))
        .collect();

    let new: Vec<&Identified> = identified
        .iter()
        .filter(|m| !known.contains(&m.uidl))
        .collect();
    let batch = request.batch_size.max(1) as usize;
    let has_more = new.len() > batch;

    let mut new_envelopes = Vec::new();
    for message in new.into_iter().take(batch) {
        let scanned = envelope_headers(wire, message, &mut top_available, timeout).await?;
        new_envelopes.push(build_envelope(message, scanned));
    }

    let outcome = SyncOutcome {
        uid_validity: None,
        highest_uid: None,
        has_more,
        new_envelopes,
        flag_updates: Vec::new(),
        removed,
    };
    Ok((outcome, SessionMap { by_uidl }))
}

/// The outcome when the mailbox is empty: no new messages, and every known
/// `UIDL` is reported as removed.
fn empty_outcome(known: &HashSet<String>) -> SyncOutcome {
    SyncOutcome {
        uid_validity: None,
        highest_uid: None,
        has_more: false,
        new_envelopes: Vec::new(),
        flag_updates: Vec::new(),
        removed: known
            .iter()
            .map(|uidl| RemoteMessageId::Pop3 { uidl: uidl.clone() })
            .collect(),
    }
}

/// Builds identities from a real `UIDL` listing, ordered by message number.
fn by_uidl(list: &[(u32, u64)], pairs: Vec<(u32, String)>) -> Vec<Identified> {
    let sizes: HashMap<u32, u64> = list.iter().copied().collect();
    let mut identified: Vec<Identified> = pairs
        .into_iter()
        .map(|(number, uidl)| Identified {
            number,
            uidl,
            size: sizes.get(&number).copied().unwrap_or(0),
            headers: None,
        })
        .collect();
    identified.sort_by_key(|m| m.number);
    identified
}

/// Builds identities by digesting each message's `TOP` headers, for servers
/// that do not support `UIDL`.
///
/// # Errors
///
/// Returns [`MailError::UnsupportedCapability`] if `TOP` is also
/// unavailable, since no stable identity can then be formed and guessing one
/// would risk duplicating or dropping messages.
async fn by_digest(
    wire: &mut crate::wire::Wire<crate::tls::Pop3TlsStream>,
    list: &[(u32, u64)],
    top_available: &mut bool,
    timeout: Duration,
) -> Result<Vec<Identified>, MailError> {
    let mut identified = Vec::with_capacity(list.len());
    for &(number, size) in list {
        let headers = fetch_top(wire, number, top_available, timeout)
            .await?
            .ok_or_else(|| MailError::UnsupportedCapability {
                capability: "UIDL or TOP".to_owned(),
            })?;
        let uidl = headers::digest_identity(&headers, size);
        identified.push(Identified {
            number,
            uidl,
            size,
            headers: Some(headers),
        });
    }
    Ok(identified)
}

/// Obtains scanned envelope headers for a new message, reusing any headers
/// fetched during digest identification and otherwise issuing `TOP`.
async fn envelope_headers(
    wire: &mut crate::wire::Wire<crate::tls::Pop3TlsStream>,
    message: &Identified,
    top_available: &mut bool,
    timeout: Duration,
) -> Result<Option<ScannedHeaders>, MailError> {
    if let Some(headers) = &message.headers {
        return Ok(Some(headers::scan(headers)));
    }
    let headers = fetch_top(wire, message.number, top_available, timeout).await?;
    Ok(headers.map(|bytes| headers::scan(&bytes)))
}

/// Issues `TOP number 0` when `TOP` may be available, caching a discovered
/// absence so it is not probed again this session.
async fn fetch_top(
    wire: &mut crate::wire::Wire<crate::tls::Pop3TlsStream>,
    number: u32,
    top_available: &mut bool,
    timeout: Duration,
) -> Result<Option<Vec<u8>>, MailError> {
    if !*top_available {
        return Ok(None);
    }
    match command::top(wire, number, 0, timeout).await? {
        Some(headers) => Ok(Some(headers)),
        None => {
            *top_available = false;
            Ok(None)
        }
    }
}

/// Builds a [`RemoteEnvelope`] from an identity and its scanned headers.
fn build_envelope(message: &Identified, scanned: Option<ScannedHeaders>) -> RemoteEnvelope {
    let (subject, from, date): (Option<String>, Vec<Address>, Option<UtcTimestamp>) = match scanned
    {
        Some(headers) => (headers.subject, headers.from, headers.date),
        None => (None, Vec::new(), None),
    };
    RemoteEnvelope {
        remote_id: RemoteMessageId::Pop3 {
            uidl: message.uidl.clone(),
        },
        subject,
        from,
        to: Vec::new(),
        date,
        flags: MessageFlags::default(),
        size_bytes: Some(message.size),
    }
}
