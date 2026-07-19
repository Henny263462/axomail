//! Archive with per-identity granularity (track 3.2 / plan-conformance).
//!
//! Archiving files each message into its account's *base* archive (the default
//! identity's `archive_dest`, else the `Archive` role). When that identity's
//! [`ArchiveGranularity`] is `Yearly`/`Monthly`, the message is filed one level
//! deeper — `Archive/YYYY` or `Archive/YYYY/MM` — keyed off the **message's own
//! date** (a deterministic source, never the wall clock), creating the dated
//! sub-folder server-first via the folder-create trait method when it does not
//! yet exist. POP3/local accounts, a `Flat` identity, an undated message, or an
//! account whose server hierarchy delimiter is unknown all fall back to the flat
//! base archive — never guessing a delimiter or a date.

use std::collections::{BTreeSet, HashMap};

use axomail_core::{
    AccountId, IncomingMailProvider, IncomingProtocol, MailError, Mailbox, MailboxId, MailboxName,
    MailboxRole, MessageId, UtcTimestamp,
};
use axomail_storage::{ArchiveGranularity, Store};
use axomail_sync::{ProviderFactory, SyncEngine};

use crate::dto::{MessageMoveDto, MoveOutcomeDto};

use super::relocate::{TargetRole, resolve_targets};
use super::{config, message_account, relocate_one, trigger_replays};

/// Archives a selection, each message into its account's archive destination,
/// honoring the default identity's [`ArchiveGranularity`] (flat / yearly /
/// monthly). Yearly/monthly file into a dated sub-folder created on demand.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if any involved account has no archive
/// destination (pre-flighted, so nothing moves), propagates a server
/// folder-create error, and returns [`MailError::Database`] on any query
/// failure.
pub async fn archive_messages_impl<P, F>(
    store: &Store,
    engine: &SyncEngine<P, F>,
    factory: &F,
    message_ids: Vec<i64>,
) -> Result<MoveOutcomeDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    // Pre-flight every account's base archive so a missing one errors before any
    // message moves (mirrors the flat-archive behavior).
    let bases = resolve_targets(store, &message_ids, TargetRole::Archive).await?;
    // Resolve (creating on demand) the concrete per-message destination, then
    // relocate each into it.
    let targets = resolve_message_targets(store, factory, &message_ids, &bases).await?;

    let mut moves = Vec::new();
    let mut accounts = BTreeSet::new();
    for id in message_ids {
        let Some(&target) = targets.get(&id) else {
            continue;
        };
        if let Some((source, op_account)) =
            relocate_one(store, MessageId::new(id), target, false).await?
        {
            moves.push(MessageMoveDto::new(id, source.value())?);
            if let Some(account_id) = op_account {
                accounts.insert(account_id);
            }
        }
    }
    trigger_replays(engine, accounts).await?;
    Ok(MoveOutcomeDto { moves })
}

/// Resolves each message's concrete archive destination, creating dated
/// sub-folders (server-first) as needed. Falls back to the flat base for a
/// `Flat` identity, a POP3/local account, an unknown delimiter, or an undated
/// message.
async fn resolve_message_targets<P, F>(
    store: &Store,
    factory: &F,
    message_ids: &[i64],
    bases: &HashMap<AccountId, MailboxId>,
) -> Result<HashMap<i64, MailboxId>, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    // Group each message (with its date) under its account.
    let mut per_account: HashMap<AccountId, Vec<(i64, Option<UtcTimestamp>)>> = HashMap::new();
    for &id in message_ids {
        let Some((envelope, account)) = message_account(store, MessageId::new(id)).await? else {
            continue;
        };
        if !bases.contains_key(&account) {
            continue;
        }
        per_account
            .entry(account)
            .or_default()
            .push((id, envelope.date));
    }

    let mut targets = HashMap::new();
    for (account, messages) in per_account {
        let base_id = bases[&account];
        let granularity = default_granularity(store, account).await?;
        let account_row = store
            .accounts()
            .get(account)
            .await?
            .ok_or_else(|| config("the message's account no longer exists"))?;
        let base = store
            .mailboxes()
            .get(base_id)
            .await?
            .ok_or_else(|| config("the archive folder no longer exists"))?;
        let delimiter = archive_delimiter(store, account, &base).await?;

        // Hierarchy only for an IMAP account with a non-flat identity and a known
        // server delimiter; otherwise everything files flat into the base.
        let hierarchical = account_row.incoming.protocol == IncomingProtocol::Imap
            && granularity != ArchiveGranularity::Flat
            && delimiter.is_some();
        if !hierarchical {
            for (id, _) in messages {
                targets.insert(id, base_id);
            }
            continue;
        }
        let delimiter = delimiter.unwrap_or_else(|| "/".to_owned());

        // One connection per account for any folder creation the batch needs.
        let mut provider = factory.make(&account_row).await?;
        provider.connect().await?;
        let resolved = ensure_dated_targets(
            store,
            &mut provider,
            account,
            &base,
            &delimiter,
            granularity,
            &messages,
        )
        .await;
        let _ = provider.disconnect().await;
        targets.extend(resolved?);
    }
    Ok(targets)
}

/// Resolves each message to its dated sub-folder id (creating the folder chain
/// as needed). An undated message files flat into `base`.
async fn ensure_dated_targets<P>(
    store: &Store,
    provider: &mut P,
    account: AccountId,
    base: &Mailbox,
    delimiter: &str,
    granularity: ArchiveGranularity,
    messages: &[(i64, Option<UtcTimestamp>)],
) -> Result<HashMap<i64, MailboxId>, MailError>
where
    P: IncomingMailProvider,
{
    let mut out = HashMap::new();
    for &(id, date) in messages {
        let Some(date) = date else {
            out.insert(id, base.id);
            continue;
        };
        let (year, month) = year_month(date.unix_seconds());
        let mut folder = ensure_child(
            store,
            provider,
            account,
            base,
            &format!("{year:04}"),
            delimiter,
        )
        .await?;
        if granularity == ArchiveGranularity::Monthly {
            folder = ensure_child(
                store,
                provider,
                account,
                &folder,
                &format!("{month:02}"),
                delimiter,
            )
            .await?;
        }
        out.insert(id, folder.id);
    }
    Ok(out)
}

/// Finds `parent`'s child folder named `leaf`, creating it server-first (then
/// locally) when absent. Idempotent within a batch: a folder created for one
/// message is found by the next.
async fn ensure_child<P>(
    store: &Store,
    provider: &mut P,
    account: AccountId,
    parent: &Mailbox,
    leaf: &str,
    delimiter: &str,
) -> Result<Mailbox, MailError>
where
    P: IncomingMailProvider,
{
    let child_remote = format!("{}{}{}", parent.remote_name.as_str(), delimiter, leaf);
    if let Some(existing) = store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find(|mailbox| mailbox.remote_name.as_str() == child_remote)
    {
        return Ok(existing);
    }
    let remote = MailboxName::new(child_remote);
    provider.create_mailbox(&remote).await?;
    store
        .mailboxes()
        .create_local(
            account,
            Some(parent.id),
            &remote,
            Some(delimiter),
            MailboxRole::Custom,
        )
        .await
}

/// The default identity's [`ArchiveGranularity`], or `Flat` when the account has
/// no identities.
async fn default_granularity(
    store: &Store,
    account: AccountId,
) -> Result<ArchiveGranularity, MailError> {
    Ok(store
        .identities()
        .list(account)
        .await?
        .into_iter()
        .find(|identity| identity.is_default)
        .map_or(ArchiveGranularity::Flat, |identity| {
            identity.fields.archive_granularity
        }))
}

/// The server hierarchy delimiter to build archive sub-folders under: the base
/// archive's own, else the first non-empty delimiter among the account's
/// folders. `None` (unknown) makes the caller keep archiving flat rather than
/// guess a separator.
async fn archive_delimiter(
    store: &Store,
    account: AccountId,
    base: &Mailbox,
) -> Result<Option<String>, MailError> {
    if let Some(delimiter) = base.delimiter.clone().filter(|d| !d.is_empty()) {
        return Ok(Some(delimiter));
    }
    Ok(store
        .mailboxes()
        .list(account)
        .await?
        .into_iter()
        .find_map(|mailbox| mailbox.delimiter.filter(|d| !d.is_empty())))
}

/// Decomposes a UTC Unix timestamp into `(year, month)` (month 1–12) using
/// Howard Hinnant's `civil_from_days`, matching the forward `days_from_civil`
/// the IMAP date parser uses — no date-time dependency, correct for pre-epoch
/// dates via flooring division.
fn year_month(unix_seconds: i64) -> (i64, u32) {
    let days = unix_seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month as u32)
}

#[cfg(test)]
mod tests {
    use super::year_month;

    #[test]
    fn year_month_decomposes_known_timestamps() {
        // 1970-01-01T00:00:00Z.
        assert_eq!(year_month(0), (1970, 1));
        // 2021-01-15T00:00:00Z (cross-checked against the IMAP date parser).
        assert_eq!(year_month(1_610_668_800), (2021, 1));
        // 2026-07-15T00:00:00Z.
        assert_eq!(year_month(1_784_073_600), (2026, 7));
        // The last second of a year stays in that year/month.
        assert_eq!(year_month(1_609_459_199), (2020, 12));
    }
}
