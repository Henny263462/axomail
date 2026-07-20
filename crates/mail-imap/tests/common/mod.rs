//! A scripted in-process IMAP server for integration tests.
//!
//! It speaks just enough IMAP to exercise the adapter: greeting, LOGIN
//! (with a `[CAPABILITY ...]` code), STARTTLS, LIST, SELECT, UID SEARCH,
//! UID FETCH (envelope / flags / body), and the mutating commands (which
//! it simply acknowledges). TLS uses an rcgen self-signed certificate
//! whose DER is handed to the client via `extra_root_certs_der`.
//!
//! Every command line the server receives is recorded so tests can assert
//! on the exact bytes sent (e.g. for `UID STORE`).
#![allow(dead_code)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{
    self, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
    BufReader,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

/// Transport the server offers.
#[derive(Clone, Copy)]
pub enum TlsMode {
    /// TLS from the first byte.
    Implicit,
    /// Plaintext until a `STARTTLS` upgrade.
    StartTls,
}

/// Fault the server injects after a successful login.
#[derive(Clone, Copy)]
pub enum Disruption {
    /// Behave normally.
    None,
    /// Read the next command but never answer (drives a client timeout).
    StallAfterLogin,
    /// Drop the connection on the next command (mid-command close).
    CloseAfterLogin,
}

/// How the scripted server responds to `IDLE` (RFC 2177), driving the push
/// tests.
#[derive(Clone, Copy)]
pub enum IdleBehavior {
    /// Acknowledge `IDLE` with a continuation, then wait for `DONE` without
    /// pushing anything. The client's wait either times out (renewal) or is
    /// interrupted (cancel), and in both cases it sends `DONE`.
    Quiet,
    /// Acknowledge `IDLE`, then push `* {0} EXISTS` and wait for `DONE`. Models
    /// new mail arriving while the client idles.
    PushExists(u32),
    /// Drop the connection the moment `IDLE` is received (before the
    /// continuation), so the client's `IDLE` init fails. Drives the
    /// reconnect-on-drop path.
    DropOnIdle,
}

/// One message the server can report.
#[derive(Clone)]
pub struct Message {
    pub uid: u32,
    pub flags: Vec<String>,
    /// Per-message mod-sequence (CONDSTORE). A `CHANGEDSINCE n` fetch only
    /// returns messages whose `modseq > n`.
    pub modseq: u64,
    pub subject: String,
    pub from_name: String,
    pub from_mailbox: String,
    pub from_host: String,
    pub to_mailbox: String,
    pub to_host: String,
    pub date: String,
    pub body: String,
}

impl Message {
    /// A minimal message with the given UID.
    pub fn new(uid: u32) -> Self {
        Self {
            uid,
            flags: vec!["\\Seen".to_owned()],
            modseq: 1,
            subject: "Test subject".to_owned(),
            from_name: String::new(),
            from_mailbox: "alice".to_owned(),
            from_host: "example.org".to_owned(),
            to_mailbox: "bob".to_owned(),
            to_host: "example.org".to_owned(),
            date: "Wed, 01 Jul 2026 12:00:00 +0000".to_owned(),
            body: format!("Subject: Test {uid}\r\n\r\nBody {uid}\r\n"),
        }
    }
}

/// Behavior specification for a server instance.
#[derive(Clone)]
pub struct Spec {
    pub tls_mode: TlsMode,
    pub username: String,
    pub password: String,
    pub capabilities: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub messages: Vec<Message>,
    pub list_lines: Vec<String>,
    pub disruption: Disruption,
    /// When set, any acknowledged command whose upper-cased form starts with
    /// this token is answered `NO` instead of `OK` (drives failure-path tests).
    pub fail_command: Option<String>,
    /// Mailbox `HIGHESTMODSEQ` reported on `SELECT` (CONDSTORE). `None` omits
    /// it entirely, which the client reads as `NOMODSEQ`.
    pub highest_modseq: Option<u64>,
    /// UIDs the server reports as `VANISHED (EARLIER)` in response to a fetch
    /// carrying the `VANISHED` modifier (QRESYNC expunge delta).
    pub vanished: Vec<u32>,
    /// When set, a `VANISHED` fetch modifier yields this raw response line
    /// (verbatim, no trailing CRLF) instead of the `vanished`-derived one —
    /// used to script malformed/ambiguous responses.
    pub vanished_override: Option<String>,
    /// How the server answers `IDLE` (RFC 2177). Only exercised by the push
    /// tests, which also add `IDLE` to [`Spec::capabilities`].
    pub idle_behavior: IdleBehavior,
}

impl Spec {
    /// A baseline spec: correct credentials `user`/`pass`, common
    /// capabilities, no messages, no disruption.
    pub fn base(tls_mode: TlsMode) -> Self {
        Self {
            tls_mode,
            username: "user".to_owned(),
            password: "pass".to_owned(),
            capabilities: "IMAP4rev1 MOVE UIDPLUS SPECIAL-USE".to_owned(),
            uid_validity: 42,
            uid_next: 1000,
            messages: Vec::new(),
            list_lines: Vec::new(),
            disruption: Disruption::None,
            fail_command: None,
            highest_modseq: None,
            vanished: Vec::new(),
            vanished_override: None,
            idle_behavior: IdleBehavior::Quiet,
        }
    }
}

/// A running server; aborts its task on drop.
pub struct TestServer {
    pub host: String,
    pub port: u16,
    pub cert_der: Vec<u8>,
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

    /// True if any recorded command contains `needle`.
    pub fn saw_command(&self, needle: &str) -> bool {
        self.commands().iter().any(|line| line.contains(needle))
    }
}

/// Starts a server bound to an ephemeral port on 127.0.0.1.
pub async fn start(spec: Spec) -> TestServer {
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(vec!["localhost".to_owned()]).expect("self-signed cert");
    let cert_der = cert.der().as_ref().to_vec();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
    let server_config = build_server_config(cert_der.clone(), key_der);
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let handle = tokio::spawn(accept_loop(listener, acceptor, spec, commands.clone()));

    TestServer {
        host: "localhost".to_owned(),
        port,
        cert_der,
        commands,
        handle,
    }
}

/// Builds the rustls server configuration from a cert/key pair.
fn build_server_config(cert_der: Vec<u8>, key_der: PrivateKeyDer<'static>) -> ServerConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert_der)], key_der)
        .expect("single cert")
}

/// Accepts connections until the task is aborted.
async fn accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    spec: Spec,
    commands: Arc<Mutex<Vec<String>>>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let acceptor = acceptor.clone();
        let spec = spec.clone();
        let commands = commands.clone();
        tokio::spawn(async move {
            let _ = handle_connection(stream, acceptor, spec, commands).await;
        });
    }
}

/// Serves one client connection.
async fn handle_connection(
    tcp: TcpStream,
    acceptor: TlsAcceptor,
    spec: Spec,
    commands: Arc<Mutex<Vec<String>>>,
) -> io::Result<()> {
    match spec.tls_mode {
        TlsMode::Implicit => {
            let tls = acceptor.accept(tcp).await?;
            let mut stream = BufReader::new(tls);
            send(&mut stream, "* OK IMAP4rev1 Service Ready\r\n").await?;
            command_loop(&mut stream, &spec, &commands).await
        }
        TlsMode::StartTls => {
            let mut plain = BufReader::new(tcp);
            send(&mut plain, "* OK IMAP4rev1 Service Ready\r\n").await?;
            if !plaintext_until_starttls(&mut plain, &spec, &commands).await? {
                return Ok(());
            }
            let tls = acceptor.accept(plain.into_inner()).await?;
            let mut stream = BufReader::new(tls);
            command_loop(&mut stream, &spec, &commands).await
        }
    }
}

/// Handles the plaintext phase, returning `true` once STARTTLS is accepted
/// (so the caller upgrades) or `false` if the client disconnected first.
async fn plaintext_until_starttls<S>(
    stream: &mut BufReader<S>,
    spec: &Spec,
    commands: &Arc<Mutex<Vec<String>>>,
) -> io::Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let Some(line) = read_line(stream).await? else {
            return Ok(false);
        };
        record(commands, &line);
        let (tag, rest) = split_tag(&line);
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("STARTTLS") {
            send(stream, &format!("{tag} OK Begin TLS negotiation now\r\n")).await?;
            return Ok(true);
        } else if upper.starts_with("CAPABILITY") {
            send_capability(stream, tag, spec).await?;
        } else {
            send(stream, &format!("{tag} BAD expected STARTTLS\r\n")).await?;
        }
    }
}

/// The main authenticated command loop.
async fn command_loop<S>(
    stream: &mut BufReader<S>,
    spec: &Spec,
    commands: &Arc<Mutex<Vec<String>>>,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut logged_in = false;
    loop {
        let Some(line) = read_line(stream).await? else {
            return Ok(());
        };
        if line.is_empty() {
            continue;
        }
        record(commands, &line);

        if logged_in {
            match spec.disruption {
                Disruption::CloseAfterLogin => return Ok(()),
                Disruption::StallAfterLogin => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return Ok(());
                }
                Disruption::None => {}
            }
        }

        let (tag, rest) = split_tag(&line);
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("LOGIN") {
            logged_in = handle_login(stream, tag, rest, spec).await?;
        } else if upper.starts_with("AUTHENTICATE XOAUTH2") {
            logged_in = handle_xoauth2(stream, tag, spec).await?;
        } else if upper.starts_with("CAPABILITY") {
            send_capability(stream, tag, spec).await?;
        } else if upper.starts_with("ENABLE") {
            // QRESYNC/CONDSTORE ENABLE: echo the enabled extensions, unless the
            // spec scripts a rejection (a server that advertises QRESYNC but
            // declines ENABLE).
            if fails(spec, &upper) {
                send(stream, &format!("{tag} NO ENABLE not supported\r\n")).await?;
            } else {
                let enabled = rest.get("ENABLE".len()..).unwrap_or("").trim();
                send(stream, &format!("* ENABLED {enabled}\r\n")).await?;
                send(stream, &format!("{tag} OK ENABLE completed\r\n")).await?;
            }
        } else if upper.starts_with("LIST") {
            for entry in &spec.list_lines {
                send(stream, entry).await?;
            }
            send(stream, &format!("{tag} OK LIST completed\r\n")).await?;
        } else if upper.starts_with("SELECT") || upper.starts_with("EXAMINE") {
            handle_select(stream, tag, &upper, spec).await?;
        } else if upper.starts_with("UID SEARCH") {
            handle_search(stream, tag, spec).await?;
        } else if upper.starts_with("UID FETCH") {
            handle_uid_fetch(stream, tag, rest, spec).await?;
        } else if upper.starts_with("APPEND") {
            handle_append(stream, tag, rest, spec).await?;
        } else if upper.starts_with("IDLE") {
            if !handle_idle(stream, tag, spec, commands).await? {
                return Ok(());
            }
        } else if upper.starts_with("LOGOUT") {
            send(stream, "* BYE Logging out\r\n").await?;
            send(stream, &format!("{tag} OK LOGOUT completed\r\n")).await?;
            return Ok(());
        } else if is_ack_command(&upper) {
            if fails(spec, &upper) {
                send(stream, &format!("{tag} NO operation rejected\r\n")).await?;
            } else {
                send(stream, &format!("{tag} OK completed\r\n")).await?;
            }
        } else {
            send(stream, &format!("{tag} BAD unknown command\r\n")).await?;
        }
    }
}

/// Commands the server merely acknowledges with a tagged OK (or a scripted NO).
fn is_ack_command(upper: &str) -> bool {
    const ACKED: [&str; 11] = [
        "UID STORE",
        "UID MOVE",
        "UID COPY",
        "UID EXPUNGE",
        "EXPUNGE",
        "NOOP",
        "CREATE",
        "DELETE",
        "RENAME",
        "SUBSCRIBE",
        "UNSUBSCRIBE",
    ];
    ACKED.iter().any(|prefix| upper.starts_with(prefix))
}

/// Whether the server should reject this command per `spec.fail_command`.
fn fails(spec: &Spec, upper: &str) -> bool {
    spec.fail_command
        .as_ref()
        .is_some_and(|token| upper.starts_with(&token.to_ascii_uppercase()))
}

/// Verifies credentials and answers LOGIN, returning whether it succeeded.
async fn handle_login<S>(
    stream: &mut BufReader<S>,
    tag: &str,
    rest: &str,
    spec: &Spec,
) -> io::Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let ok = parse_two_quoted(rest)
        .is_some_and(|(user, pass)| user == spec.username && pass == spec.password);
    if ok {
        send(
            stream,
            &format!(
                "{tag} OK [CAPABILITY {}] LOGIN completed\r\n",
                spec.capabilities
            ),
        )
        .await?;
    } else {
        send(
            stream,
            &format!("{tag} NO [AUTHENTICATIONFAILED] credentials rejected\r\n"),
        )
        .await?;
    }
    Ok(ok)
}

/// Handles `AUTHENTICATE XOAUTH2`: prompt with an empty continuation, read the
/// base64 SASL initial response, and accept it when its bearer token matches
/// `spec.password` (reused here as the expected access token).
async fn handle_xoauth2<S>(stream: &mut BufReader<S>, tag: &str, spec: &Spec) -> io::Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send(stream, "+ \r\n").await?;
    let Some(response) = read_line(stream).await? else {
        return Ok(false);
    };
    let ok = xoauth2_bearer_token(&response).as_deref() == Some(spec.password.as_str());
    if ok {
        send(
            stream,
            &format!(
                "{tag} OK [CAPABILITY {}] XOAUTH2 authenticated\r\n",
                spec.capabilities
            ),
        )
        .await?;
    } else {
        send(
            stream,
            &format!("{tag} NO [AUTHENTICATIONFAILED] token rejected\r\n"),
        )
        .await?;
    }
    Ok(ok)
}

/// Extracts the bearer token from a base64 XOAUTH2 SASL string
/// (`user=..\x01auth=Bearer <token>\x01\x01`).
fn xoauth2_bearer_token(base64_response: &str) -> Option<String> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64_response.trim())
        .ok()?;
    let text = String::from_utf8_lossy(&decoded);
    text.split('\u{1}')
        .find_map(|part| part.strip_prefix("auth=Bearer ").map(str::to_owned))
}

/// Sends a CAPABILITY response.
async fn send_capability<S>(stream: &mut BufReader<S>, tag: &str, spec: &Spec) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    send(stream, &format!("* CAPABILITY {}\r\n", spec.capabilities)).await?;
    send(stream, &format!("{tag} OK CAPABILITY completed\r\n")).await
}

/// Sends a SELECT response advertising UIDVALIDITY/UIDNEXT (and, when the
/// select requested CONDSTORE/QRESYNC, the mailbox mod-sequence).
async fn handle_select<S>(
    stream: &mut BufReader<S>,
    tag: &str,
    upper: &str,
    spec: &Spec,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let condstore = upper.contains("CONDSTORE") || upper.contains("QRESYNC");
    send(stream, &format!("* {} EXISTS\r\n", spec.messages.len())).await?;
    send(stream, "* 0 RECENT\r\n").await?;
    send(
        stream,
        &format!("* OK [UIDVALIDITY {}] UIDs valid\r\n", spec.uid_validity),
    )
    .await?;
    send(
        stream,
        &format!("* OK [UIDNEXT {}] predicted next\r\n", spec.uid_next),
    )
    .await?;
    send(
        stream,
        "* FLAGS (\\Seen \\Answered \\Flagged \\Deleted \\Draft)\r\n",
    )
    .await?;
    // CONDSTORE: only a mod-sequence-enabling select advertises HIGHESTMODSEQ
    // (or NOMODSEQ when the mailbox has none); a plain select stays untouched.
    if condstore {
        match spec.highest_modseq {
            Some(modseq) => {
                send(
                    stream,
                    &format!("* OK [HIGHESTMODSEQ {modseq}] Highest mod-sequence\r\n"),
                )
                .await?;
            }
            None => {
                send(stream, "* OK [NOMODSEQ] No mod-sequences\r\n").await?;
            }
        }
    }
    send(
        stream,
        &format!("{tag} OK [READ-WRITE] SELECT completed\r\n"),
    )
    .await
}

/// Sends `* SEARCH` with the UIDs of all current messages.
async fn handle_search<S>(stream: &mut BufReader<S>, tag: &str, spec: &Spec) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let uids: Vec<String> = spec.messages.iter().map(|m| m.uid.to_string()).collect();
    send(stream, &format!("* SEARCH {}\r\n", uids.join(" "))).await?;
    send(stream, &format!("{tag} OK SEARCH completed\r\n")).await
}

/// Answers a UID FETCH, choosing envelope/flags/body form from the query.
async fn handle_uid_fetch<S>(
    stream: &mut BufReader<S>,
    tag: &str,
    rest: &str,
    spec: &Spec,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut parts = rest.splitn(4, ' ');
    parts.next();
    parts.next();
    let set = parts.next().unwrap_or("");
    let query = parts.next().unwrap_or("").to_ascii_uppercase();
    let max_uid = spec.messages.iter().map(|m| m.uid).max().unwrap_or(0);
    // CONDSTORE `CHANGEDSINCE n` restricts the reply to messages whose
    // mod-sequence exceeds `n`.
    let changed_since = changedsince_arg(&query);

    for (idx, msg) in spec.messages.iter().enumerate() {
        if !uid_in_set(set, msg.uid, max_uid) {
            continue;
        }
        if let Some(floor) = changed_since
            && msg.modseq <= floor
        {
            continue;
        }
        let seq = idx + 1;
        if query.contains("BODY") {
            send_body(stream, seq, msg).await?;
        } else if query.contains("ENVELOPE") {
            send(stream, &envelope_response(seq, msg)).await?;
        } else {
            send(stream, &flags_response(seq, msg)).await?;
        }
    }
    // QRESYNC: a `VANISHED` fetch modifier draws a `VANISHED (EARLIER)` line
    // naming the expunged UIDs (or a scripted override for malformed cases).
    if query.contains("VANISHED") {
        if let Some(line) = &spec.vanished_override {
            send(stream, &format!("{line}\r\n")).await?;
        } else if !spec.vanished.is_empty() {
            let uids: Vec<String> = spec.vanished.iter().map(u32::to_string).collect();
            send(
                stream,
                &format!("* VANISHED (EARLIER) {}\r\n", uids.join(",")),
            )
            .await?;
        }
    }
    send(stream, &format!("{tag} OK FETCH completed\r\n")).await
}

/// Handles `APPEND mailbox [(flags)] {len}`: reads the sync literal after a `+`
/// continuation and acknowledges, or rejects (before the literal) when the spec
/// scripts an `APPEND` failure.
///
/// The command line itself is already recorded, so a test can assert on the
/// mailbox and flags; the literal bytes are read and discarded to keep the
/// stream framed.
async fn handle_append<S>(
    stream: &mut BufReader<S>,
    tag: &str,
    rest: &str,
    spec: &Spec,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if fails(spec, &rest.to_ascii_uppercase()) {
        // Reject before the literal: with a sync literal the client waits for a
        // continuation and never sends the body after a tagged NO.
        send(stream, &format!("{tag} NO APPEND rejected\r\n")).await?;
        return Ok(());
    }
    let Some(len) = append_literal_len(rest) else {
        send(stream, &format!("{tag} BAD malformed APPEND literal\r\n")).await?;
        return Ok(());
    };
    // Continuation request, then consume the literal plus its trailing CRLF.
    send(stream, "+ OK send the message\r\n").await?;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    let mut crlf = [0u8; 2];
    stream.read_exact(&mut crlf).await?;
    send(
        stream,
        &format!("{tag} OK [APPENDUID 42 1000] APPEND completed\r\n"),
    )
    .await
}

/// Handles `IDLE` (RFC 2177) per [`Spec::idle_behavior`].
///
/// Returns `true` once the client's `DONE` has been acknowledged (so the
/// command loop continues, allowing renewal with a second `IDLE`), or `false`
/// if the connection should close (a scripted drop, or the client hanging up).
/// The `DONE` line the client sends is recorded so tests can assert on it.
async fn handle_idle<S>(
    stream: &mut BufReader<S>,
    tag: &str,
    spec: &Spec,
    commands: &Arc<Mutex<Vec<String>>>,
) -> io::Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if let IdleBehavior::DropOnIdle = spec.idle_behavior {
        // Drop before the continuation so the client's IDLE init fails.
        return Ok(false);
    }
    send(stream, "+ idling\r\n").await?;
    if let IdleBehavior::PushExists(count) = spec.idle_behavior {
        // A short pause so the client's IDLE init completes on the continuation
        // before the unsolicited EXISTS arrives (mirrors mail landing mid-idle).
        tokio::time::sleep(Duration::from_millis(30)).await;
        send(stream, &format!("* {count} EXISTS\r\n")).await?;
    }
    // Wait for the client's untagged `DONE`, then acknowledge, ending the IDLE.
    loop {
        let Some(line) = read_line(stream).await? else {
            return Ok(false);
        };
        record(commands, &line);
        if line.trim().eq_ignore_ascii_case("DONE") {
            send(stream, &format!("{tag} OK IDLE terminated\r\n")).await?;
            return Ok(true);
        }
    }
}

/// Parses the `{len}` (or `{len+}`) sync-literal size at the end of an APPEND
/// command line.
fn append_literal_len(rest: &str) -> Option<usize> {
    let open = rest.rfind('{')?;
    let close = rest[open..].find('}')? + open;
    rest[open + 1..close].trim_end_matches('+').parse().ok()
}

/// Extracts the numeric argument of a `CHANGEDSINCE n` fetch modifier, if any.
fn changedsince_arg(query: &str) -> Option<u64> {
    let idx = query.find("CHANGEDSINCE")?;
    query[idx + "CHANGEDSINCE".len()..]
        .split_whitespace()
        .next()
        .and_then(|token| token.trim_end_matches(')').parse().ok())
}

/// Builds an ENVELOPE FETCH response line for a message.
fn envelope_response(seq: usize, msg: &Message) -> String {
    format!(
        "* {seq} FETCH (UID {uid} RFC822.SIZE {size} INTERNALDATE \"01-Jul-2026 12:00:00 +0000\" \
         FLAGS ({flags}) ENVELOPE ({date} {subject} {from} NIL NIL {to} NIL NIL NIL NIL))\r\n",
        uid = msg.uid,
        size = msg.body.len(),
        flags = msg.flags.join(" "),
        date = nstring(&msg.date),
        subject = nstring(&msg.subject),
        from = address_list(&msg.from_name, &msg.from_mailbox, &msg.from_host),
        to = address_list("", &msg.to_mailbox, &msg.to_host),
    )
}

/// Builds a FLAGS-only FETCH response line.
fn flags_response(seq: usize, msg: &Message) -> String {
    format!(
        "* {seq} FETCH (UID {uid} FLAGS ({flags}))\r\n",
        uid = msg.uid,
        flags = msg.flags.join(" "),
    )
}

/// Sends a BODY[] FETCH response using an IMAP literal.
async fn send_body<S>(stream: &mut BufReader<S>, seq: usize, msg: &Message) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut resp = String::new();
    resp.push_str("* ");
    resp.push_str(&seq.to_string());
    resp.push_str(" FETCH (UID ");
    resp.push_str(&msg.uid.to_string());
    resp.push_str(" BODY[] {");
    resp.push_str(&msg.body.len().to_string());
    resp.push_str("}\r\n");
    resp.push_str(&msg.body);
    resp.push_str(")\r\n");
    send(stream, &resp).await
}

/// Formats a one-address parenthesized list, or the empty list.
fn address_list(name: &str, mailbox: &str, host: &str) -> String {
    format!(
        "(({} NIL {} {}))",
        nstring(name),
        nstring(mailbox),
        nstring(host)
    )
}

/// Renders an IMAP nstring: a quoted string, or `NIL` when empty.
fn nstring(value: &str) -> String {
    if value.is_empty() {
        "NIL".to_owned()
    } else {
        format!("\"{value}\"")
    }
}

/// Tests whether `uid` falls in an IMAP sequence set, treating `*` as
/// `max_uid`.
fn uid_in_set(set: &str, uid: u32, max_uid: u32) -> bool {
    set.split(',').any(|part| match part.split_once(':') {
        Some((a, b)) => {
            let lo = bound(a, max_uid);
            let hi = bound(b, max_uid);
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            uid >= lo && uid <= hi
        }
        None => bound(part, max_uid) == uid,
    })
}

/// Resolves a sequence-set bound token (`*` or a number).
fn bound(token: &str, max_uid: u32) -> u32 {
    if token == "*" {
        max_uid
    } else {
        token.parse().unwrap_or(0)
    }
}

/// Extracts the two quoted arguments of a LOGIN command.
fn parse_two_quoted(rest: &str) -> Option<(String, String)> {
    let mut parts = rest.split('"');
    parts.next();
    let user = parts.next()?.to_owned();
    parts.next();
    let pass = parts.next()?.to_owned();
    Some((user, pass))
}

/// Splits a command line into its tag and the remainder.
fn split_tag(line: &str) -> (&str, &str) {
    line.split_once(' ').unwrap_or((line, ""))
}

/// Appends a received command line to the shared recorder.
fn record(commands: &Arc<Mutex<Vec<String>>>, line: &str) {
    if let Ok(mut guard) = commands.lock() {
        guard.push(line.to_owned());
    }
}

/// Reads one CRLF-terminated line, or `None` at end of stream.
async fn read_line<S>(stream: &mut S) -> io::Result<Option<String>>
where
    S: AsyncBufRead + Unpin,
{
    let mut buf = Vec::new();
    let read = stream.read_until(b'\n', &mut buf).await?;
    if read == 0 {
        return Ok(None);
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

/// Writes `data` and flushes it.
async fn send<S>(stream: &mut S, data: &str) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    stream.write_all(data.as_bytes()).await?;
    stream.flush().await
}
