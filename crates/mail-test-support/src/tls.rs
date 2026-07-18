//! Self-signed TLS identity for the local test servers.
//!
//! [`TestTlsIdentity::generate`] mints a fresh single-certificate identity
//! with `rcgen` and hands back both a ready-to-serve rustls
//! [`ServerConfig`] and the certificate's DER bytes. Clients trust the
//! server by feeding [`TestTlsIdentity::root_der`] into their own
//! extra-roots hook (or by calling [`TestTlsIdentity::client_config`],
//! which builds a matching [`ClientConfig`] directly).

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::error::TestServerError;

/// Subject alternative names baked into the generated certificate. Both a
/// DNS name and the loopback IP are present so clients may validate against
/// either `localhost` or `127.0.0.1`.
const SUBJECT_ALT_NAMES: [&str; 2] = ["localhost", "127.0.0.1"];

/// A freshly generated, self-signed TLS identity for a test server.
///
/// The identity is its own trust anchor: because the certificate is
/// self-signed, [`root_der`](Self::root_der) is exactly the certificate a
/// client must trust to validate the server.
#[derive(Clone)]
pub struct TestTlsIdentity {
    /// Server configuration presenting the generated certificate.
    server_config: Arc<ServerConfig>,
    /// DER encoding of the self-signed certificate, usable as a client-side
    /// trust anchor.
    root_der: Vec<u8>,
}

impl TestTlsIdentity {
    /// Generates a new identity with a fresh key pair and certificate.
    ///
    /// # Errors
    ///
    /// Returns [`TestServerError::CertGen`] if `rcgen` cannot build the
    /// certificate and [`TestServerError::Tls`] if rustls rejects the
    /// resulting key material.
    pub fn generate() -> Result<Self, TestServerError> {
        let sans: Vec<String> = SUBJECT_ALT_NAMES.iter().map(|s| (*s).to_owned()).collect();
        let certified = rcgen::generate_simple_self_signed(sans)
            .map_err(|err| TestServerError::CertGen(err.to_string()))?;

        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.signing_key.serialize_der();

        let cert_chain = vec![CertificateDer::from(cert_der.clone())];
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));

        let server_config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|err| TestServerError::Tls(err.to_string()))?
                .with_no_client_auth()
                .with_single_cert(cert_chain, private_key)
                .map_err(|err| TestServerError::Tls(err.to_string()))?;

        Ok(Self {
            server_config: Arc::new(server_config),
            root_der: cert_der,
        })
    }

    /// Shared server configuration presenting this identity's certificate.
    #[must_use]
    pub fn server_config(&self) -> Arc<ServerConfig> {
        Arc::clone(&self.server_config)
    }

    /// DER bytes of the self-signed certificate, for use as a client trust
    /// anchor via an extra-roots hook.
    #[must_use]
    pub fn root_der(&self) -> &[u8] {
        &self.root_der
    }

    /// Builds a [`TlsAcceptor`] for wrapping accepted server-side sockets.
    #[must_use]
    pub fn acceptor(&self) -> TlsAcceptor {
        TlsAcceptor::from(self.server_config())
    }

    /// Builds a rustls [`ClientConfig`] that trusts exactly this identity.
    ///
    /// # Errors
    ///
    /// Returns [`TestServerError::Tls`] if the certificate cannot be added
    /// to the root store or a protocol version cannot be selected.
    pub fn client_config(&self) -> Result<ClientConfig, TestServerError> {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(self.root_der.clone()))
            .map_err(|err| TestServerError::Tls(err.to_string()))?;

        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|err| TestServerError::Tls(err.to_string()))?
                .with_root_certificates(roots)
                .with_no_client_auth();

        Ok(config)
    }

    /// Convenience wrapper around [`client_config`](Self::client_config)
    /// yielding a ready [`TlsConnector`].
    ///
    /// # Errors
    ///
    /// Propagates any error from [`client_config`](Self::client_config).
    pub fn connector(&self) -> Result<TlsConnector, TestServerError> {
        Ok(TlsConnector::from(Arc::new(self.client_config()?)))
    }
}
