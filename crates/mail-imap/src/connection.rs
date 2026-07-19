//! Establishing an authenticated IMAP session.
//!
//! Handles TCP connect, the implicit-TLS and `STARTTLS` variants, `LOGIN`,
//! and capability discovery. `STARTTLS` always completes **before** any
//! credentials are sent.

use std::collections::HashSet;
use std::sync::Arc;

use async_imap::Authenticator;
use async_imap::Client;
use async_imap::types::{Capabilities, Capability};
use axomail_core::{ConnectionCredential, ConnectionSecurity, MailError, xoauth2_sasl};
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
    /// Whether `ENABLE QRESYNC` succeeded on this connection. QRESYNC is enabled
    /// lazily by the sync path (see [`crate::condstore`]) and never on the
    /// write-back connection, so per-message `EXPUNGE`s keep their plain
    /// semantics. Only a genuinely enabled connection trusts `VANISHED`
    /// removals.
    pub qresync_enabled: bool,
    /// Whether `ENABLE QRESYNC` has already been *attempted* on this connection
    /// (regardless of outcome). RFC 3501 forbids `ENABLE` outside the
    /// authenticated-not-selected state, so once a server declines it the sync
    /// path must not re-issue it on every subsequent (selected) pass; it degrades
    /// to CONDSTORE-only for the life of the connection.
    pub qresync_attempted: bool,
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

/// Authenticates according to the configured credential and records the
/// resulting capability set.
///
/// Password accounts use `LOGIN` (which can piggyback the post-login
/// capabilities); OAuth2 accounts use `AUTHENTICATE XOAUTH2` and then issue an
/// explicit `CAPABILITY`, since the SASL exchange returns no capability list.
/// `STARTTLS` has already completed before this runs, so credentials are never
/// sent in the clear.
async fn login(config: &ImapConfig, client: Client<ImapTlsStream>) -> Result<Connected, MailError> {
    match &config.credential {
        ConnectionCredential::Password(password) => login_password(config, client, password).await,
        ConnectionCredential::XOauth2 { access_token } => {
            login_xoauth2(config, client, access_token).await
        }
    }
}

/// Password login via `LOGIN`, capturing any capabilities the server returns.
async fn login_password(
    config: &ImapConfig,
    client: Client<ImapTlsStream>,
    password: &str,
) -> Result<Connected, MailError> {
    let (mut session, caps) = with_deadline(config.timeout, async {
        client
            .login_with_capabilities(&config.username, password)
            .await
            .map_err(|(err, _client)| map_login_error(&err))
    })
    .await?;

    let capabilities = match caps {
        Some(caps) => capability_set(&caps),
        None => fetch_capabilities(config, &mut session).await?,
    };

    Ok(Connected {
        session,
        capabilities,
        qresync_enabled: false,
        qresync_attempted: false,
    })
}

/// OAuth2 login via `AUTHENTICATE XOAUTH2`, then an explicit `CAPABILITY`.
async fn login_xoauth2(
    config: &ImapConfig,
    client: Client<ImapTlsStream>,
    access_token: &str,
) -> Result<Connected, MailError> {
    let authenticator = XOauth2Authenticator::new(&config.username, access_token);
    let mut session = with_deadline(config.timeout, async {
        client
            .authenticate("XOAUTH2", authenticator)
            .await
            .map_err(|(err, _client)| map_login_error(&err))
    })
    .await?;
    let capabilities = fetch_capabilities(config, &mut session).await?;
    Ok(Connected {
        session,
        capabilities,
        qresync_enabled: false,
        qresync_attempted: false,
    })
}

/// Issues `CAPABILITY` on an authenticated session and collects the atoms.
async fn fetch_capabilities(
    config: &ImapConfig,
    session: &mut ImapSession,
) -> Result<HashSet<String>, MailError> {
    let caps = with_deadline(config.timeout, async {
        session
            .capabilities()
            .await
            .map_err(|err| map_imap_error(&err))
    })
    .await?;
    Ok(capability_set(&caps))
}

/// SASL `XOAUTH2` authenticator: emits the initial client response to the
/// first (empty) server challenge and nothing thereafter, so a provider's
/// base64 error challenge does not get answered with the credential again.
struct XOauth2Authenticator {
    response: Vec<u8>,
    sent: bool,
}

impl XOauth2Authenticator {
    fn new(username: &str, access_token: &str) -> Self {
        Self {
            response: xoauth2_sasl(username, access_token).into_bytes(),
            sent: false,
        }
    }
}

impl Authenticator for XOauth2Authenticator {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        if self.sent {
            return Vec::new();
        }
        self.sent = true;
        self.response.clone()
    }
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
