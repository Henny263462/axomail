//! Mapping of transport failures onto [`MailError`].
//!
//! Every helper here is careful about secrets: raw server responses (which
//! could echo credentials or opaque tokens) are never copied into an error
//! message. Only fixed, hand-written descriptions and the [`io::ErrorKind`]
//! category are surfaced (see the crate-level docs and ADR-0003).

use std::future::Future;
use std::io;
use std::time::Duration;

use axomail_core::MailError;

/// Runs `fut` under a deadline, mapping expiry to [`MailError::Timeout`].
///
/// Every network round-trip in this crate goes through this helper so that
/// a stalled server can never hang a sync worker indefinitely.
pub(crate) async fn with_deadline<T, F>(timeout: Duration, fut: F) -> Result<T, MailError>
where
    F: Future<Output = Result<T, MailError>> + Send,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(result) => result,
        Err(_elapsed) => Err(MailError::Timeout { after: timeout }),
    }
}

/// Maps a TLS-handshake failure onto [`MailError::Tls`].
///
/// Certificate problems must always surface here and are classified
/// permanent: they are never retried or downgraded (ADR-0003).
pub(crate) fn map_tls_error(err: &io::Error) -> MailError {
    MailError::Tls {
        message: redact_io(err),
    }
}

/// Maps a socket-level I/O error onto [`MailError::Network`].
///
/// These socket errors are connection-level and transient; only the
/// [`io::ErrorKind`] category (never the raw message) is surfaced.
pub(crate) fn map_io_error(err: &io::Error) -> MailError {
    let message = if err.kind() == io::ErrorKind::TimedOut {
        "the connection timed out".to_owned()
    } else {
        redact_io(err)
    };
    MailError::Network { message }
}

/// Maps a name-resolution failure onto either [`MailError::Configuration`]
/// (a definitive "no such host", usually a typo) or [`MailError::Dns`]
/// (a transient lookup failure that may be worth retrying), following the
/// guidance on the variants in `axomail-core`.
pub(crate) fn map_resolution_error(host: &str, err: &io::Error) -> MailError {
    let text = err.to_string().to_ascii_lowercase();
    const NOT_FOUND_MARKERS: [&str; 5] = [
        "not known",
        "not found",
        "no such host",
        "nodename nor servname",
        "name or service not known",
    ];
    if NOT_FOUND_MARKERS.iter().any(|m| text.contains(m)) {
        MailError::Configuration {
            message: format!("the server name '{host}' does not exist; check it for typos"),
        }
    } else {
        MailError::Dns {
            host: host.to_owned(),
        }
    }
}

/// Produces a short, credential-free description of an I/O error.
///
/// Only the [`io::ErrorKind`] category is used; the (potentially sensitive
/// or noisy) inner message is not included.
fn redact_io(err: &io::Error) -> String {
    format!("{:?}", err.kind())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test]
    async fn deadline_maps_expiry_to_timeout() {
        let result: Result<(), MailError> = with_deadline(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        })
        .await;
        assert!(matches!(result, Err(MailError::Timeout { .. })));
    }

    #[tokio::test]
    async fn deadline_passes_through_ready_value() {
        let result: Result<u8, MailError> =
            with_deadline(Duration::from_secs(5), async { Ok(7) }).await;
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn timed_out_io_becomes_network() {
        let err = io::Error::from(io::ErrorKind::TimedOut);
        assert!(matches!(map_io_error(&err), MailError::Network { .. }));
    }

    #[test]
    fn resolution_not_found_becomes_configuration() {
        let io_err = io::Error::other("nodename nor servname provided");
        assert!(matches!(
            map_resolution_error("pop.example.org", &io_err),
            MailError::Configuration { .. }
        ));
    }

    #[test]
    fn resolution_generic_becomes_dns() {
        let io_err = io::Error::other("temporary failure");
        assert!(matches!(
            map_resolution_error("pop.example.org", &io_err),
            MailError::Dns { .. }
        ));
    }
}
