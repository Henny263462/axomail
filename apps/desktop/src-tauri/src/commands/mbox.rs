//! Folder mbox export / import commands (Track 4.6).
//!
//! **Export** streams a folder's messages to a dialog-chosen mbox file: each
//! message's raw `.eml` is sourced exactly like the reading pane
//! ([`load_raw_for_export`] — cached, fetched on demand when online, honouring
//! the offline gate) and framed by [`crate::mbox::write_message`]. Only one
//! message's bytes are resident at a time; a message whose body cannot be
//! sourced (for example a remote message not cached while offline) is skipped
//! and counted, never fatal.
//!
//! **Import** reads a dialog-chosen mbox file (or a single `.eml`) and ingests
//! each message into a chosen folder as a **local** message (no server): the
//! file is split on a blocking task and messages flow over a bounded channel to
//! the async ingest, so a large archive is never fully resident. Both follow
//! the ADR-0010 rule — the dialog picks the path, Rust does the IO; the webview
//! gains no filesystem scope.

use std::io::{BufRead, Read};
use std::path::Path;

use axomail_core::{IncomingMailProvider, MailError, MailboxId, MessageFlags};
use axomail_parser::parse_message;
use axomail_storage::{MessageCursor, NewMessage, Store, raw_message_path, write_raw_atomically};
use axomail_sync::ProviderFactory;
use tokio::io::AsyncWriteExt;

use crate::commands::detail::load_raw_for_export;
use crate::commands::map_command_error;
use crate::dto::{MboxExportDto, MboxImportDto};
use crate::mbox::{MboxReader, write_message};
use crate::state::AppState;

/// Page size for walking a folder's messages during export.
const EXPORT_PAGE_LIMIT: u32 = 200;

/// Postmark metadata for one message, gathered before its body is streamed.
struct ExportMeta {
    id: i64,
    sender: String,
    date: Option<i64>,
}

/// Exports every message of `mailbox_id` to the mbox file at `dest_path`.
///
/// Messages are written oldest-first. Each body is sourced via
/// [`load_raw_for_export`] (offline-gated); one that cannot be sourced is
/// skipped and reported in [`MboxExportDto::skipped`] rather than failing the
/// whole export.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder and
/// [`MailError::Filesystem`] on a write error, and propagates database errors
/// from paging the folder.
pub async fn export_folder_mbox_impl<P, F>(
    store: &Store,
    factory: &F,
    raw_root: &Path,
    offline: bool,
    mailbox_id: i64,
    dest_path: &Path,
) -> Result<MboxExportDto, MailError>
where
    P: IncomingMailProvider + 'static,
    F: ProviderFactory<P>,
{
    let mailbox = MailboxId::new(mailbox_id);
    store
        .mailboxes()
        .get(mailbox)
        .await?
        .ok_or_else(|| config("the folder does not exist"))?;

    let metas = collect_export_metadata(store, mailbox).await?;

    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|error| fs_error("creating the mbox file", &error))?;
    let mut exported = 0usize;
    let mut skipped = 0usize;
    for meta in &metas {
        match load_raw_for_export(store, factory, raw_root, meta.id, offline).await {
            Ok(raw) => {
                let mut framed = Vec::with_capacity(raw.len() + 128);
                // Writing into a `Vec` is infallible (its `Write` never errors).
                let _ = write_message(&mut framed, &meta.sender, meta.date, &raw);
                file.write_all(&framed)
                    .await
                    .map_err(|error| fs_error("writing the mbox file", &error))?;
                exported += 1;
            }
            Err(error) => {
                tracing::warn!(message_id = meta.id, %error, "skipping a message during mbox export");
                skipped += 1;
            }
        }
    }
    file.flush()
        .await
        .map_err(|error| fs_error("finalizing the mbox file", &error))?;
    MboxExportDto::new(exported, skipped)
}

/// Gathers export postmark metadata (id, sender, date) for the folder,
/// oldest-first. Only metadata is held in memory; bodies stream later.
async fn collect_export_metadata(
    store: &Store,
    mailbox: MailboxId,
) -> Result<Vec<ExportMeta>, MailError> {
    let mut metas = Vec::new();
    let mut cursor: Option<MessageCursor> = None;
    loop {
        let page = store
            .messages()
            .list_page(mailbox, cursor, EXPORT_PAGE_LIMIT)
            .await?;
        for envelope in &page.items {
            metas.push(ExportMeta {
                id: envelope.id.value(),
                sender: envelope
                    .from
                    .first()
                    .map(|address| address.email.as_str().to_owned())
                    .unwrap_or_default(),
                date: envelope.date.map(|date| date.unix_seconds()),
            });
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    // `list_page` yields newest-first; mbox convention is oldest-first.
    metas.reverse();
    Ok(metas)
}

/// Imports an mbox file (or a single `.eml`) at `src_path` into `mailbox_id`,
/// ingesting each message as a local message. Returns the count ingested.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] for an unknown folder and propagates
/// parse / filesystem / database errors. A read error mid-stream leaves the
/// already-ingested prefix committed (a partial import).
pub async fn import_mbox_impl(
    store: &Store,
    raw_root: &Path,
    mailbox_id: i64,
    src_path: &Path,
) -> Result<MboxImportDto, MailError> {
    let mailbox = MailboxId::new(mailbox_id);
    store
        .mailboxes()
        .get(mailbox)
        .await?
        .ok_or_else(|| config("the target folder does not exist"))?;

    // Split on a blocking task; messages flow over a bounded channel so a large
    // archive is never fully resident in memory.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    let path = src_path.to_path_buf();
    let reader = tokio::task::spawn_blocking(move || split_source(&path, &tx));

    let mut imported = 0usize;
    while let Some(raw) = rx.recv().await {
        ingest_local(store, raw_root, mailbox, &raw).await?;
        imported += 1;
    }
    reader.await.map_err(|_| MailError::Filesystem {
        message: "the mbox import task failed".to_owned(),
    })??;
    MboxImportDto::new(imported)
}

/// Splits the source file into raw messages, sending each over `tx`. An mbox
/// (first bytes are a `From ` postmark) is streamed message-by-message; any
/// other file is treated as a single `.eml`.
fn split_source(path: &Path, tx: &tokio::sync::mpsc::Sender<Vec<u8>>) -> Result<(), MailError> {
    let file =
        std::fs::File::open(path).map_err(|error| fs_error("opening the import file", &error))?;
    let mut reader = std::io::BufReader::new(file);
    let looks_like_mbox = reader
        .fill_buf()
        .map_err(|error| fs_error("reading the import file", &error))?
        .starts_with(b"From ");
    if looks_like_mbox {
        let mut mbox = MboxReader::new(reader);
        while let Some(message) = mbox.next_message()? {
            if message.is_empty() {
                continue;
            }
            if tx.blocking_send(message).is_err() {
                break; // the async consumer was dropped (an ingest error upstream)
            }
        }
    } else {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| fs_error("reading the import file", &error))?;
        if !bytes.is_empty() {
            let _ = tx.blocking_send(bytes);
        }
    }
    Ok(())
}

/// Ingests one raw message as a local message in `mailbox`: parse the envelope,
/// insert the row, and cache the raw bytes under `<raw_root>/raw/<id>.eml`.
async fn ingest_local(
    store: &Store,
    raw_root: &Path,
    mailbox: MailboxId,
    raw: &[u8],
) -> Result<(), MailError> {
    let parsed = parse_message(raw)?;
    let new = NewMessage {
        mailbox,
        remote_id: None,
        subject: parsed.subject.clone(),
        from: parsed.from.clone(),
        to: parsed.to.clone(),
        cc: parsed.cc.clone(),
        bcc: Vec::new(),
        date: parsed.date,
        flags: MessageFlags::default(),
        size_bytes: Some(u64::try_from(raw.len()).unwrap_or(u64::MAX)),
        preview: Some(parsed.preview.clone()),
        has_raw: true,
    };
    let envelope = store.messages().insert(&new).await?;
    let path = raw_message_path(raw_root, envelope.id.value());
    write_raw_atomically(&path, raw).await
}

/// Maps an IO error to a redacted [`MailError::Filesystem`] (kind only).
fn fs_error(action: &str, error: &std::io::Error) -> MailError {
    MailError::Filesystem {
        message: format!("{action}: {}", error.kind()),
    }
}

/// Shorthand for a user-facing configuration error.
fn config(message: &str) -> MailError {
    MailError::Configuration {
        message: message.to_owned(),
    }
}

// --- Tauri command wrappers ------------------------------------------------

/// Exports a folder to a dialog-chosen mbox file.
#[tauri::command]
pub async fn export_folder_mbox(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
    dest_path: String,
) -> Result<MboxExportDto, String> {
    export_folder_mbox_impl(
        state.store(),
        state.factory(),
        state.raw_root(),
        state.is_offline(),
        mailbox_id,
        Path::new(&dest_path),
    )
    .await
    .map_err(map_command_error)
}

/// Imports a dialog-chosen mbox / `.eml` file into a folder.
#[tauri::command]
pub async fn import_mbox(
    state: tauri::State<'_, AppState>,
    mailbox_id: i64,
    src_path: String,
) -> Result<MboxImportDto, String> {
    import_mbox_impl(
        state.store(),
        state.raw_root(),
        mailbox_id,
        Path::new(&src_path),
    )
    .await
    .map_err(map_command_error)
}
