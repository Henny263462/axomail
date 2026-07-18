//! TLS client configuration for SMTP.
//!
//! Builds a [`TlsConnector`] whose trust anchors are the public
//! [`webpki-roots`](webpki_roots) bundle plus any extra DER roots the caller
//! supplied for the local test server. Certificate verification is always the
//! rustls default; there is no path that accepts invalid certificates (see the
//! crate-level docs).

use std::sync::Arc;

use axomail_core::MailError;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

/// Builds a strict [`TlsConnector`] trusting the public roots plus `extra_roots`.
///
/// # Errors
///
/// Returns [`MailError::Tls`] if an extra root certificate cannot be parsed or
/// if the rustls provider rejects the selected protocol versions. TLS setup
/// failures are never downgraded (ADR-0003).
pub(crate) fn build_tls_connector(extra_roots: &[Vec<u8>]) -> Result<TlsConnector, MailError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    for der in extra_roots {
        let cert = CertificateDer::from(der.clone());
        roots.add(cert).map_err(|err| MailError::Tls {
            message: format!("additional root certificate is invalid: {err}"),
        })?;
    }

    // Pin the crypto provider explicitly (ring) so the connector never depends
    // on a process-wide default being installed elsewhere.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|err| MailError::Tls {
            message: format!("TLS protocol configuration failed: {err}"),
        })?
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}
