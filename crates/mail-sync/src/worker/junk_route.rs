//! Auto-routing of ingest-scored junk during a sync pass.
//!
//! The ingest transaction scores freshly arrived inbound mail and returns the
//! ids that crossed the junk threshold (see
//! [`IngestSummary::auto_junk_candidates`](axomail_storage::IngestSummary)).
//! This module files each of them with the shared [`crate::junk::route_to_junk`]
//! disposition, then wakes an `OpReplay` per affected account so the queued
//! `$Junk` keyword and Junk-folder move reach the server promptly. Routing
//! failures are logged, never fatal: a junk hiccup must not fail a sync.

use std::collections::BTreeSet;
use std::sync::Arc;

use axomail_core::{IncomingMailProvider, MessageId};

use crate::factory::ProviderFactory;
use crate::shared::Shared;

/// Files `candidates` as junk and triggers one `OpReplay` per affected account.
pub(super) async fn autoroute<P, F>(shared: &Arc<Shared<P, F>>, candidates: &[MessageId])
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mut accounts = BTreeSet::new();
    for &message_id in candidates {
        match crate::junk::route_to_junk(&shared.store, message_id).await {
            Ok(Some(account)) => {
                accounts.insert(account);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(error = %error, "auto-junk routing failed for a message");
            }
        }
    }
    // Re-drain enqueues an OpReplay only when the account has claimable ops,
    // which the junk keyword/move ops just created.
    for account in accounts {
        super::redrain_pending_ops(shared, account).await;
    }
}
