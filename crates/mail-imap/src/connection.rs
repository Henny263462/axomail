//! Establishing an authenticated IMAP session.
//!
//! Handles TCP connect, the implicit-TLS and `STARTTLS` variants, `LOGIN`,
//! and capability discovery. `STARTTLS` always completes **before** any
//! credentials are sent.

use std::collections::HashSet;
use std::sync::Arc;

use async_imap::Client;
use async_imap::types::{Capabilities, Capability};
use axomail_core::{ConnectionSecurity, MailError};
use rustls::ClientConfig;
use tokio::net::TcpStream;

use crate::config::ImapConfig;
use crate::error::{
    map_imap_error, map_io_error, map_login_error, map_resolution_error, with_deadline,
};
use crate::tls::{ImapTlsStream, build_client_config, handshake};

/// An authenticated IMAP session over TLS.
pub(crate) type ImapSession = async_imap::Session<ImapTlsStream>;

/// An open, authenticated session together with the capabilities the
/// server advertised (stored upper-cased for case-insensitive lookup).
pub(crate) struct Connected {
    /// The authenticated session.
    pub session: ImapSession,
    /// Upper-cased capability atoms (e.g. `MOVE`, `UIDPLUS`, `SPECIAL-USE`).
    pub capabilities: HashSet<String>,
}

impl Connected {
    /// Returns `true` if the server advertised the given capability.
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.contains(&name.to_ascii_uppercase())
    }
}

/// Opens a connection and logs in according to `config`.
pub(crate) async fn connect(config: &ImapConfig) -> Result<Connected, MailError> {
    let host = config.endpoint.host();
    tracing::debug!(
        host,
        port = config.endpoint.port().get(),
        "opening IMAP connection"
    );
    let tls_config = Arc::new(build_client_config(&config.extra_root_certs_der)?);

    let tcp = connect_tcp(config).await?;
    let client = match config.security {
        ConnectionSecurity::Tls => implicit_tls(config, tls_config, host, tcp).await?,
        ConnectionSecurity::StartTls => start_tls(config, tls_config, host, tcp).await?,
    };
    let connected = login(config, client).await?;
    tracing::debug!(
        capabilities = connected.capabilities.len(),
        "IMAP session authenticated"
    );
    Ok(connected)
}

/// Resolves the host and opens a TCP connection to the first address that
/// accepts, mapping resolution and connection failures distinctly.
async fn connect_tcp(config: &ImapConfig) -> Result<TcpStream, MailError> {
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

/// Implicit-TLS path: handshake first, then read the greeting.
async fn implicit_tls(
    config: &ImapConfig,
    tls_config: Arc<ClientConfig>,
    host: &str,
    tcp: TcpStream,
) -> Result<Client<ImapTlsStream>, MailError> {
    let tls = with_deadline(config.timeout, handshake(tls_config, host, tcp)).await?;
    let mut client = Client::new(tls);
    read_greeting(config, &mut client).await?;
    Ok(client)
}

/// `STARTTLS` path: greeting on the plaintext channel, upgrade, then no
/// second greeting (per RFC 3501).
async fn start_tls(
    config: &ImapConfig,
    tls_config: Arc<ClientConfig>,
    host: &str,
    tcp: TcpStream,
) -> Result<Client<ImapTlsStream>, MailError> {
    let mut plain = Client::new(tcp);
    read_greeting(config, &mut plain).await?;
    with_deadline(config.timeout, async {
        plain
            .run_command_and_check_ok("STARTTLS", None)
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    let inner = plain.into_inner();
    let tls = with_deadline(config.timeout, handshake(tls_config, host, inner)).await?;
    Ok(Client::new(tls))
}

/// Reads and discards the server greeting, failing if the stream closes
/// before one arrives.
async fn read_greeting<T>(config: &ImapConfig, client: &mut Client<T>) -> Result<(), MailError>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    with_deadline(config.timeout, async {
        match client.read_response().await {
            Ok(Some(_greeting)) => Ok(()),
            Ok(None) => Err(MailError::Protocol {
                message: "the server closed the connection before greeting".to_owned(),
            }),
            Err(err) => Err(map_io_error(&err)),
        }
    })
    .await
}

/// Sends `LOGIN` and records the resulting capability set.
async fn login(config: &ImapConfig, client: Client<ImapTlsStream>) -> Result<Connected, MailError> {
    let (mut session, caps) = with_deadline(config.timeout, async {
        client
            .login_with_capabilities(&config.username, &config.password)
            .await
            .map_err(|(err, _client)| map_login_error(&err))
    })
    .await?;

    let capabilities = match caps {
        Some(caps) => capability_set(&caps),
        None => {
            let caps = with_deadline(config.timeout, async {
                session
                    .capabilities()
                    .await
                    .map_err(|err| map_imap_error(&err))
            })
            .await?;
            capability_set(&caps)
        }
    };

    Ok(Connected {
        session,
        capabilities,
    })
}

/// Converts async-imap capabilities into an upper-cased string set.
fn capability_set(caps: &Capabilities) -> HashSet<String> {
    caps.iter()
        .map(|cap| match cap {
            Capability::Imap4rev1 => "IMAP4REV1".to_owned(),
            Capability::Auth(value) => format!("AUTH={}", value.to_ascii_uppercase()),
            Capability::Atom(value) => value.to_ascii_uppercase(),
        })
        .collect()
}
