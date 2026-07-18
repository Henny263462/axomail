//! TLS setup for IMAP connections.
//!
//! Uses rustls with the Mozilla root set (via `webpki-roots`) plus any
//! extra DER trust anchors from the configuration. There is deliberately
//! no code path that disables certificate verification.

use std::sync::Arc;

use axomail_core::MailError;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::error::map_tls_error;

/// The single concrete stream type carried by every session: a TLS layer
/// over a TCP socket, whether reached directly or via `STARTTLS`.
pub(crate) type ImapTlsStream = TlsStream<TcpStream>;

/// Builds a rustls client configuration trusting the built-in Mozilla
/// roots plus `extra_root_certs_der`.
///
/// # Errors
///
/// Returns [`MailError::Tls`] if the crypto provider cannot be set up or
/// an extra certificate is not valid DER.
pub(crate) fn build_client_config(
    extra_root_certs_der: &[Vec<u8>],
) -> Result<ClientConfig, MailError> {
    let mut roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    for der in extra_root_certs_der {
        let cert = CertificateDer::from(der.clone());
        roots.add(cert).map_err(|_| MailError::Tls {
            message: "an extra root certificate was not valid DER".to_owned(),
        })?;
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| MailError::Tls {
            message: "could not negotiate a TLS protocol version".to_owned(),
        })?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

/// Parses `host` into a rustls server name used for SNI and certificate
/// verification.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] when the host is not a valid DNS
/// name or IP literal.
pub(crate) fn server_name(host: &str) -> Result<ServerName<'static>, MailError> {
    ServerName::try_from(host.to_owned()).map_err(|_| MailError::Configuration {
        message: "the server host is not a valid TLS server name".to_owned(),
    })
}

/// Performs the TLS handshake over an already-connected TCP stream.
///
/// # Errors
///
/// Handshake and certificate failures map to [`MailError::Tls`] and are
/// never retried.
pub(crate) async fn handshake(
    config: Arc<ClientConfig>,
    host: &str,
    tcp: TcpStream,
) -> Result<ImapTlsStream, MailError> {
    let connector = TlsConnector::from(config);
    let name = server_name(host)?;
    connector
        .connect(name, tcp)
        .await
        .map_err(|err| map_tls_error(&err))
}
