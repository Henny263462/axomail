//! Establishing an authenticated POP3 session.
//!
//! Handles TCP connect, the implicit-TLS and `STLS` variants, `CAPA`, and
//! `USER`/`PASS` authentication. For `STLS` the TLS upgrade always completes
//! **before** any credentials are sent, and the adapter refuses to continue
//! in plaintext if the server does not offer it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axomail_core::{ConnectionCredential, ConnectionSecurity, MailError};
use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::command;
use crate::config::Pop3Config;
use crate::error::{map_io_error, map_resolution_error, with_deadline};
use crate::tls::{Pop3TlsStream, build_client_config, handshake};
use crate::wire::{Reply, Wire};

/// Capabilities advertised by the server via `CAPA`.
pub(crate) struct Capabilities {
    /// Whether `CAPA` succeeded at all; `false` means the server answered
    /// `-ERR` and nothing is known.
    known: bool,
    /// Upper-cased capability atoms (e.g. `UIDL`, `TOP`, `STLS`).
    atoms: HashSet<String>,
}

impl Capabilities {
    /// Builds capabilities from a `CAPA` result.
    fn from_atoms(atoms: Option<Vec<String>>) -> Self {
        match atoms {
            Some(list) => Self {
                known: true,
                atoms: list.into_iter().collect(),
            },
            None => Self {
                known: false,
                atoms: HashSet::new(),
            },
        }
    }

    /// Whether the server advertised the given (upper-cased) capability.
    pub(crate) fn has(&self, name: &str) -> bool {
        self.atoms.contains(name)
    }

    /// Whether the capability list is trustworthy (i.e. `CAPA` succeeded).
    pub(crate) fn known(&self) -> bool {
        self.known
    }
}

/// An open, authenticated POP3 session plus its advertised capabilities.
pub(crate) struct Connected {
    wire: Wire<Pop3TlsStream>,
    caps: Capabilities,
}

impl Connected {
    /// Mutable access to the session wire for issuing commands.
    pub(crate) fn wire(&mut self) -> &mut Wire<Pop3TlsStream> {
        &mut self.wire
    }

    /// Whether `TOP` was advertised. Only meaningful when [`caps_known`] is
    /// `true`.
    ///
    /// [`caps_known`]: Self::caps_known
    pub(crate) fn has_top(&self) -> bool {
        self.caps.has("TOP")
    }

    /// Whether the capability list is trustworthy.
    pub(crate) fn caps_known(&self) -> bool {
        self.caps.known()
    }

    /// Consumes the session, returning the wire for a graceful `QUIT`.
    pub(crate) fn into_wire(self) -> Wire<Pop3TlsStream> {
        self.wire
    }
}

/// Opens a connection and authenticates according to `config`.
pub(crate) async fn connect(config: &Pop3Config) -> Result<Connected, MailError> {
    let host = config.endpoint.host();
    tracing::debug!(
        host,
        port = config.endpoint.port().get(),
        "opening POP3 connection"
    );
    let tls_config = Arc::new(build_client_config(&config.extra_root_certs_der)?);
    let tcp = connect_tcp(config).await?;

    match config.security {
        ConnectionSecurity::Tls => implicit_tls(config, tls_config, host, tcp).await,
        ConnectionSecurity::StartTls => start_tls(config, tls_config, host, tcp).await,
    }
}

/// Implicit-TLS path: handshake first, then greeting, `CAPA`, and auth.
async fn implicit_tls(
    config: &Pop3Config,
    tls_config: Arc<ClientConfig>,
    host: &str,
    tcp: TcpStream,
) -> Result<Connected, MailError> {
    let tls = with_deadline(config.timeout, handshake(tls_config, host, tcp)).await?;
    let mut wire = Wire::new(tls);
    read_greeting(&mut wire, config.timeout).await?;
    let caps = Capabilities::from_atoms(command::capabilities(&mut wire, config.timeout).await?);
    authenticate(config, &mut wire, &caps).await?;
    tracing::debug!("POP3 session authenticated (implicit TLS)");
    Ok(Connected { wire, caps })
}

/// `STLS` path: greeting and `CAPA` in plaintext (used **only** to confirm
/// `STLS` support), upgrade, re-issue `CAPA` over TLS, then auth. Credentials
/// are never sent on the plaintext channel.
async fn start_tls(
    config: &Pop3Config,
    tls_config: Arc<ClientConfig>,
    host: &str,
    tcp: TcpStream,
) -> Result<Connected, MailError> {
    let mut plain = Wire::new(tcp);
    read_greeting(&mut plain, config.timeout).await?;
    let plain_caps =
        Capabilities::from_atoms(command::capabilities(&mut plain, config.timeout).await?);
    if plain_caps.known() && !plain_caps.has("STLS") {
        return Err(MailError::UnsupportedCapability {
            capability: "STLS".to_owned(),
        });
    }
    command::stls(&mut plain, config.timeout).await?;

    // Hand the raw socket to the TLS handshake. Any bytes the server sent
    // after the `+OK` but before TLS would be unauthenticated injection, so
    // a non-empty buffer here is a protocol violation.
    let (tcp, leftover) = plain.into_parts();
    if !leftover.is_empty() {
        return Err(MailError::Protocol {
            message: "the server sent data before the TLS handshake".to_owned(),
        });
    }

    let tls = with_deadline(config.timeout, handshake(tls_config, host, tcp)).await?;
    let mut wire = Wire::new(tls);
    // The plaintext CAPA is unauthenticated, and many servers only advertise
    // SASL mechanisms (e.g. XOAUTH2) once TLS is active. Re-query capabilities
    // over the encrypted channel and use those for authentication and for the
    // returned session; the plaintext list is discarded here.
    let caps = Capabilities::from_atoms(command::capabilities(&mut wire, config.timeout).await?);
    authenticate(config, &mut wire, &caps).await?;
    tracing::debug!("POP3 session authenticated (STLS)");
    Ok(Connected { wire, caps })
}

/// Authenticates according to the configured credential.
///
/// Password accounts use `USER`/`PASS`. OAuth2 accounts use `AUTH XOAUTH2`;
/// when the capability list is known and does not advertise `SASL`, the
/// mechanism is rejected up front with a clear [`MailError::UnsupportedCapability`]
/// rather than a blind attempt.
async fn authenticate<S>(
    config: &Pop3Config,
    wire: &mut Wire<S>,
    caps: &Capabilities,
) -> Result<(), MailError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    match &config.credential {
        ConnectionCredential::Password(password) => {
            command::authenticate(wire, &config.username, password, config.timeout).await
        }
        ConnectionCredential::XOauth2 { access_token } => {
            if caps.known() && !caps.has("SASL") {
                return Err(MailError::UnsupportedCapability {
                    capability: "SASL XOAUTH2".to_owned(),
                });
            }
            command::authenticate_xoauth2(wire, &config.username, access_token, config.timeout)
                .await
        }
    }
}

/// Resolves the host and opens a TCP connection to the first address that
/// accepts, mapping resolution and connection failures distinctly.
async fn connect_tcp(config: &Pop3Config) -> Result<TcpStream, MailError> {
    let host = config.endpoint.host();
    let port = config.endpoint.port().get();

    let addrs = with_deadline(config.timeout, async {
        tokio::net::lookup_host((host, port))
            .await
            .map(|iter| iter.collect::<Vec<_>>())
            .map_err(|err| map_resolution_error(host, &err))
    })
    .await?;

    if addrs.is_empty() {
        return Err(MailError::Configuration {
            message: format!("the server name '{host}' did not resolve to any address"),
        });
    }

    let mut last_err = None;
    for addr in addrs {
        let attempt = with_deadline(config.timeout, async {
            TcpStream::connect(addr)
                .await
                .map_err(|err| map_io_error(&err))
        })
        .await;
        match attempt {
            Ok(stream) => return Ok(stream),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or(MailError::Network {
        message: "could not connect to the server".to_owned(),
    }))
}

/// Reads the service greeting, requiring a `+OK`.
async fn read_greeting<S>(wire: &mut Wire<S>, timeout: Duration) -> Result<(), MailError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    with_deadline(timeout, async {
        match wire.read_status().await? {
            Reply::Ok(_) => Ok(()),
            Reply::Err => Err(MailError::Protocol {
                message: "the server declined the connection at greeting".to_owned(),
            }),
        }
    })
    .await
}
