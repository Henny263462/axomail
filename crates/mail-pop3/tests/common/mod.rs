//! A scripted in-process POP3 server for integration tests.
//!
//! It speaks just enough POP3 to exercise the adapter: greeting, `CAPA`,
//! `STLS` (with a real TLS upgrade), `USER`/`PASS`, `STAT`, `LIST`, `UIDL`,
//! `TOP`, `RETR`, `DELE`, and `QUIT`. Capability advertisement and the
//! `UIDL`/`TOP`/`STLS` behaviours are individually toggleable so tests can
//! drive the fallback paths. TLS uses an `rcgen` self-signed identity from
//! `axomail-test-support`, handed to the client via `extra_root_certs_der`.
//!
//! Every command line received is recorded for assertions (e.g. that `DELE`
//! was or was not sent).
#![allow(dead_code)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::num::NonZeroU16;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{ConnectionSecurity, ServerEndpoint};
use axomail_pop3::Pop3Config;
use axomail_test_support::TestTlsIdentity;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Transport the server offers.
#[derive(Clone, Copy)]
pub enum TlsMode {
    /// TLS from the first byte.
    Implicit,
    /// Plaintext until an `STLS` upgrade.
    StartTls,
}

/// Fault the server injects after a successful login.
#[derive(Clone, Copy)]
pub enum Disruption {
    /// Behave normally.
    None,
    /// Read the next command but never answer (drives a client timeout).
    StallAfterAuth,
    /// Drop the connection on the next command (mid-command close).
    CloseAfterAuth,
}

/// A message the server can report.
#[derive(Clone)]
pub struct TestMessage {
    pub uidl: String,
    /// Complete RFC 5322 message with CRLF line endings.
    pub full: String,
}

impl TestMessage {
    /// Builds a small message with the given `UIDL` and header values.
    pub fn new(uidl: &str, subject: &str, from: &str) -> Self {
        let full = format!(
            "From: {from}\r\nTo: bob@example.org\r\nSubject: {subject}\r\n\
             Date: Tue, 14 Jul 2026 09:15:00 +0000\r\n\r\nBody of {uidl}\r\n"
        );
        Self {
            uidl: uidl.to_owned(),
            full,
        }
    }

    /// Octet size as reported by `STAT`/`LIST`.
    fn size(&self) -> usize {
        self.full.len()
    }

    /// The header block returned by `TOP n 0`: everything up to and
    /// including the blank separator line.
    fn header_block(&self) -> String {
        match self.full.find("\r\n\r\n") {
            Some(idx) => self.full[..idx + 4].to_owned(),
            None => self.full.clone(),
        }
    }
}

/// Behaviour specification for a server instance.
#[derive(Clone)]
pub struct Spec {
    pub tls_mode: TlsMode,
    pub username: String,
    pub password: String,
    pub messages: Vec<TestMessage>,
    pub advertise_stls: bool,
    pub advertise_uidl: bool,
    pub advertise_top: bool,
    pub uidl_ok: bool,
    pub top_ok: bool,
    /// Advertise `SASL XOAUTH2` — but only in a `CAPA` issued **over TLS**, so a
    /// client that trusts the plaintext capability list would never see it.
    pub advertise_sasl: bool,
    pub disruption: Disruption,
}

impl Spec {
    /// A baseline spec: credentials `user`/`pass`, `UIDL` and `TOP`
    /// supported and advertised, no disruption.
    pub fn base(tls_mode: TlsMode) -> Self {
        Self {
            tls_mode,
            username: "user".to_owned(),
            password: "pass".to_owned(),
            messages: Vec::new(),
            advertise_stls: matches!(tls_mode, TlsMode::StartTls),
            advertise_uidl: true,
            advertise_top: true,
            uidl_ok: true,
            top_ok: true,
            advertise_sasl: false,
            disruption: Disruption::None,
        }
    }
}

/// A running server; aborts its task on drop.
pub struct TestServer {
    pub host: String,
    pub port: u16,
    pub root_der: Vec<u8>,
    commands: Arc<Mutex<Vec<String>>>,
    handle: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl TestServer {
    /// Every command line received so far.
    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().expect("commands lock").clone()
    }

    /// True if any recorded command starts with `needle` (case-insensitive).
    pub fn saw_command(&self, needle: &str) -> bool {
        let upper = needle.to_ascii_uppercase();
        self.commands()
            .iter()
            .any(|line| line.to_ascii_uppercase().starts_with(&upper))
    }

    /// Builds a provider configuration that trusts this server's cert.
    pub fn config(&self, security: ConnectionSecurity, password: &str) -> Pop3Config {
        let endpoint = ServerEndpoint::new(
            self.host.clone(),
            NonZeroU16::new(self.port).expect("nonzero port"),
        )
        .expect("valid endpoint");
        let mut config = Pop3Config::new(endpoint, security, "user", password);
        config.extra_root_certs_der = vec![self.root_der.clone()];
        config.timeout = Duration::from_secs(5);
        config
    }
}

/// Starts a server bound to an ephemeral port on 127.0.0.1.
pub async fn start(spec: Spec) -> TestServer {
    let identity = TestTlsIdentity::generate().expect("tls identity");
    let root_der = identity.root_der().to_vec();
    let identity = Arc::new(identity);

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let handle = tokio::spawn(accept_loop(listener, identity, spec, commands.clone()));

    TestServer {
        host: "127.0.0.1".to_owned(),
        port,
        root_der,
        commands,
        handle,
    }
}

/// Accepts connections until the task is aborted.
async fn accept_loop(
    listener: TcpListener,
    identity: Arc<TestTlsIdentity>,
    spec: Spec,
    commands: Arc<Mutex<Vec<String>>>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let identity = identity.clone();
        let spec = spec.clone();
        let commands = commands.clone();
        tokio::spawn(async move {
            let _ = handle_connection(stream, identity, spec, commands).await;
        });
    }
}

/// Serves one client connection.
async fn handle_connection(
    tcp: TcpStream,
    identity: Arc<TestTlsIdentity>,
    spec: Spec,
    commands: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    match spec.tls_mode {
        TlsMode::Implicit => {
            let acceptor = identity.acceptor();
            let mut tls = acceptor.accept(tcp).await?;
            write_line(&mut tls, "+OK axomail POP3 ready").await?;
            command_loop(&mut tls, &spec, &commands).await
        }
        TlsMode::StartTls => {
            let mut tcp = tcp;
            write_line(&mut tcp, "+OK axomail POP3 ready").await?;
            if !authorization_plain(&mut tcp, &spec, &commands).await? {
                return Ok(());
            }
            let acceptor = identity.acceptor();
            let mut tls = acceptor.accept(tcp).await?;
            command_loop(&mut tls, &spec, &commands).await
        }
    }
}

/// Handles the plaintext phase, returning `true` once `STLS` is accepted
/// (so the caller upgrades) or `false` if the client left first.
async fn authorization_plain<S>(
    stream: &mut S,
    spec: &Spec,
    commands: &Arc<Mutex<Vec<String>>>,
) -> std::io::Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let Some(line) = read_line(stream).await? else {
            return Ok(false);
        };
        record(commands, &line);
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("CAPA") {
            // Plaintext phase: never advertise SASL here.
            send_capa(stream, spec, false).await?;
        } else if upper.starts_with("STLS") {
            if spec.advertise_stls {
                write_line(stream, "+OK begin TLS negotiation").await?;
                return Ok(true);
            }
            write_line(stream, "-ERR TLS not available").await?;
        } else if upper.starts_with("QUIT") {
            write_line(stream, "+OK bye").await?;
            return Ok(false);
        } else {
            write_line(stream, "-ERR issue STLS first").await?;
        }
    }
}

/// The main command loop over the (possibly upgraded) stream.
async fn command_loop<S>(
    stream: &mut S,
    spec: &Spec,
    commands: &Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut logged_in = false;
    loop {
        let Some(line) = read_line(stream).await? else {
            return Ok(());
        };
        record(commands, &line);

        if logged_in {
            match spec.disruption {
                Disruption::CloseAfterAuth => return Ok(()),
                Disruption::StallAfterAuth => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return Ok(());
                }
                Disruption::None => {}
            }
        }

        let upper = line.to_ascii_uppercase();
        if upper.starts_with("CAPA") {
            // This loop only runs over TLS (implicit, or post-STLS upgrade), so
            // SASL may be advertised here.
            send_capa(stream, spec, true).await?;
        } else if upper.starts_with("AUTH XOAUTH2") {
            // Accept the SASL initial-response form outright; the adapter only
            // checks for a leading `+OK`.
            write_line(stream, "+OK maildrop ready").await?;
            logged_in = true;
        } else if upper.starts_with("USER") {
            write_line(stream, "+OK user accepted").await?;
        } else if upper.starts_with("PASS") {
            let password = line.strip_prefix("PASS ").unwrap_or("");
            if password == spec.password {
                write_line(stream, "+OK mailbox ready").await?;
                logged_in = true;
            } else {
                write_line(stream, "-ERR invalid credentials").await?;
            }
        } else if upper.starts_with("STAT") {
            let total: usize = spec.messages.iter().map(TestMessage::size).sum();
            write_line(stream, &format!("+OK {} {total}", spec.messages.len())).await?;
        } else if upper.starts_with("LIST") {
            send_list(stream, spec).await?;
        } else if upper.starts_with("UIDL") {
            send_uidl(stream, spec).await?;
        } else if upper.starts_with("TOP") {
            send_top(stream, spec, &line).await?;
        } else if upper.starts_with("RETR") {
            send_retr(stream, spec, &line).await?;
        } else if upper.starts_with("DELE") {
            write_line(stream, "+OK marked for deletion").await?;
        } else if upper.starts_with("QUIT") {
            write_line(stream, "+OK axomail POP3 signing off").await?;
            return Ok(());
        } else {
            write_line(stream, "-ERR unknown command").await?;
        }
    }
}

/// Sends a `CAPA` reply reflecting the advertised capabilities. `tls_active`
/// gates TLS-only advertisements: `SASL` is only ever listed once the channel
/// is encrypted, mirroring servers that hide their SASL mechanisms in plaintext.
async fn send_capa<S>(stream: &mut S, spec: &Spec, tls_active: bool) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut lines = vec!["USER".to_owned()];
    if spec.advertise_uidl {
        lines.push("UIDL".to_owned());
    }
    if spec.advertise_top {
        lines.push("TOP".to_owned());
    }
    if spec.advertise_stls {
        lines.push("STLS".to_owned());
    }
    if spec.advertise_sasl && tls_active {
        lines.push("SASL XOAUTH2".to_owned());
    }
    send_multiline(stream, "+OK capability list follows", &lines).await
}

/// Sends a `LIST` scan listing (`number size` per message).
async fn send_list<S>(stream: &mut S, spec: &Spec) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let lines: Vec<String> = spec
        .messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| format!("{} {}", idx + 1, msg.size()))
        .collect();
    send_multiline(
        stream,
        &format!("+OK {} messages", spec.messages.len()),
        &lines,
    )
    .await
}

/// Sends a `UIDL` listing, or `-ERR` when unsupported.
async fn send_uidl<S>(stream: &mut S, spec: &Spec) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !spec.uidl_ok {
        return write_line(stream, "-ERR UIDL not supported").await;
    }
    let lines: Vec<String> = spec
        .messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| format!("{} {}", idx + 1, msg.uidl))
        .collect();
    send_multiline(stream, "+OK unique-id listing follows", &lines).await
}

/// Sends a `TOP` header block, or `-ERR` when unsupported.
async fn send_top<S>(stream: &mut S, spec: &Spec, line: &str) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !spec.top_ok {
        return write_line(stream, "-ERR TOP not supported").await;
    }
    let Some(msg) = message_for(spec, line) else {
        return write_line(stream, "-ERR no such message").await;
    };
    let block = msg.header_block();
    write_line(stream, "+OK top follows").await?;
    write_body(stream, &block).await
}

/// Sends a `RETR` message body.
async fn send_retr<S>(stream: &mut S, spec: &Spec, line: &str) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(msg) = message_for(spec, line) else {
        return write_line(stream, "-ERR no such message").await;
    };
    write_line(stream, &format!("+OK {} octets", msg.size())).await?;
    write_body(stream, &msg.full).await
}

/// Resolves the one-based message number in `LIST`/`TOP`/`RETR`/`DELE`.
fn message_for<'a>(spec: &'a Spec, line: &str) -> Option<&'a TestMessage> {
    let number: usize = line.split_whitespace().nth(1)?.parse().ok()?;
    spec.messages.get(number.checked_sub(1)?)
}

/// Records a received command line.
fn record(commands: &Arc<Mutex<Vec<String>>>, line: &str) {
    if let Ok(mut guard) = commands.lock() {
        guard.push(line.to_owned());
    }
}

/// Writes a multi-line reply: intro line, dot-stuffed body lines, `.`.
async fn send_multiline<S>(stream: &mut S, intro: &str, lines: &[String]) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_line(stream, intro).await?;
    let mut out = String::new();
    for line in lines {
        push_dot_stuffed(&mut out, line);
    }
    out.push_str(".\r\n");
    write_raw(stream, out.as_bytes()).await
}

/// Writes a message/header body as a dot-stuffed, dot-terminated block.
async fn write_body<S>(stream: &mut S, body: &str) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut out = String::new();
    for line in split_lines(body) {
        push_dot_stuffed(&mut out, line);
    }
    out.push_str(".\r\n");
    write_raw(stream, out.as_bytes()).await
}

/// Appends a dot-stuffed line with a trailing CRLF.
fn push_dot_stuffed(out: &mut String, line: &str) {
    if line.starts_with('.') {
        out.push('.');
    }
    out.push_str(line);
    out.push_str("\r\n");
}

/// Splits a body into lines (tolerating `\r\n`/`\n`), dropping a single
/// trailing empty line so a body does not gain a spurious blank line.
fn split_lines(body: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = body
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

/// Writes a single CRLF-terminated line.
async fn write_line<S>(stream: &mut S, text: &str) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    write_raw(stream, text.as_bytes()).await?;
    write_raw(stream, b"\r\n").await
}

/// Writes raw bytes and flushes.
async fn write_raw<S>(stream: &mut S, bytes: &[u8]) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    stream.write_all(bytes).await?;
    stream.flush().await
}

/// Reads one command line byte-by-byte, so no bytes are buffered past the
/// terminator (needed for the clean `STLS` handoff). Returns `None` at EOF.
async fn read_line<S>(stream: &mut S) -> std::io::Result<Option<String>>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        let read = stream.read(&mut byte).await?;
        if read == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
    }
    while matches!(buf.last(), Some(b'\r')) {
        buf.pop();
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}
