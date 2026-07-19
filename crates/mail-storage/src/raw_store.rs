//! On-disk cache of raw RFC 5322 message bytes.
//!
//! Raw bodies live outside the database under `<raw_root>/raw/<message_id>.eml`
//! (ADR-0001: only metadata and a `has_raw` marker sit in SQLite). This module
//! is the single source of truth for that layout and its read/write/remove
//! primitives — the reading pane's on-demand fetch, the sync-time offline
//! download, and account-deletion cleanup all go through it, so the path
//! scheme can never drift between them.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axomail_core::MailError;

/// Path of the cached raw message for `message_id`
/// (`<raw_root>/raw/<message_id>.eml`).
#[must_use]
pub fn raw_message_path(raw_root: &Path, message_id: i64) -> PathBuf {
    raw_root.join("raw").join(format!("{message_id}.eml"))
}

/// Reads the cached raw bytes for a message, returning `None` when nothing is
/// cached yet.
///
/// # Errors
///
/// Returns [`MailError::Filesystem`] on any read error other than "not found".
pub async fn read_raw(path: &Path) -> Result<Option<Vec<u8>>, MailError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(MailError::Filesystem {
            message: format!("reading cached message: {}", error.kind()),
        }),
    }
}

/// Writes `bytes` to `path` atomically: create the directory, write a uniquely
/// named temp file beside the target, then rename over it. A concurrent reader
/// therefore never observes a half-written body.
///
/// # Errors
///
/// Returns [`MailError::Filesystem`] if the directory, temp write, or rename
/// fails, or if `path` has no parent directory.
pub async fn write_raw_atomically(path: &Path, bytes: &[u8]) -> Result<(), MailError> {
    let parent = path.parent().ok_or_else(|| MailError::Filesystem {
        message: "raw cache path has no parent directory".to_owned(),
    })?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("creating raw cache directory: {}", error.kind()),
        })?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp = path.with_extension(format!("eml.{nanos}.tmp"));
    tokio::fs::write(&temp, bytes)
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("writing raw cache: {}", error.kind()),
        })?;
    tokio::fs::rename(&temp, path)
        .await
        .map_err(|error| MailError::Filesystem {
            message: format!("finalizing raw cache: {}", error.kind()),
        })
}

/// Removes the cached raw body at `path` if present, best-effort.
///
/// A missing file is success: cleanup and policy purges never need the body to
/// have existed. Returns `true` when a file was actually removed.
///
/// # Errors
///
/// Returns [`MailError::Filesystem`] on a removal error other than "not found".
pub async fn remove_raw(path: &Path) -> Result<bool, MailError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(MailError::Filesystem {
            message: format!("removing cached message: {}", error.kind()),
        }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn path_is_raw_subdir_keyed_by_id() {
        let root = Path::new("/data");
        let path = raw_message_path(root, 42);
        assert!(path.ends_with("raw/42.eml") || path.ends_with("raw\\42.eml"));
    }

    #[tokio::test]
    async fn write_then_read_then_remove_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = raw_message_path(dir.path(), 7);
        assert_eq!(read_raw(&path).await.unwrap(), None);

        write_raw_atomically(&path, b"hello").await.unwrap();
        assert_eq!(
            read_raw(&path).await.unwrap().as_deref(),
            Some(&b"hello"[..])
        );

        assert!(remove_raw(&path).await.unwrap());
        assert_eq!(read_raw(&path).await.unwrap(), None);
        // A second remove is a no-op, not an error.
        assert!(!remove_raw(&path).await.unwrap());
    }
}
