//! Integration tests for the SMTP submission adapter.
//!
//! Each test drives [`axomail_smtp::SmtpProvider`] against a scripted, in-process
//! SMTP server (real TCP, real TLS via a self-signed rcgen certificate). The
//! server speaks enough of RFC 5321 to exercise the adapter: greeting, EHLO,
//! AUTH PLAIN/LOGIN, STARTTLS, MAIL FROM, RCPT TO, DATA capture, and QUIT, with
//! scriptable failure replies and a stall mode for the timeout path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axomail_core::{
    Address, ConnectionSecurity, EmailAddress, MailError, OutgoingAttachment, OutgoingMailProvider,
    OutgoingMessage, SendResult, ServerEndpoint,
};
use axomail_smtp::{SmtpConfig, SmtpProvider};
use mail_parser::{MessageParser, MimeHeaders};
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

const GREETING: &str = "220 test.local ESMTP axomail-test";
const OK_RCPT: &str = "250 2.1.5 Recipient OK";
const OK_DATA: &str = "250 2.0.0 Message accepted for delivery";

// Binary attachment payload; the leading bytes look like a PDF, the tail is
// non-UTF-8 so the test also proves base64 transfer-encoding round-trips.
const PDF_BYTES: &[u8] = &[
    0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x00, 0x01, 0xff, 0xfe, 0x80,
];

// ---------------------------------------------------------------------------
// Scripted server
// ---------------------------------------------------------------------------

/// What the scripted server should do for one connection.
#[derive(Clone)]
struct Script {
    security: ConnectionSecurity,
    auth_succeeds: bool,
    rcpt_reply: String,
    data_reply: String,
    stall: bool,
}

impl Script {
    fn tls() -> Self {
        Self {
            security: ConnectionSecurity::Tls,
            auth_succeeds: true,
            rcpt_reply: OK_RCPT.to_owned(),
            data_reply: OK_DATA.to_owned(),
            stall: false,
        }
    }

    fn starttls() -> Self {
        Self {
            security: ConnectionSecurity::StartTls,
            ..Self::tls()
        }
    }
}

/// Everything the server observed during the session.
#[derive(Default)]
struct Capture {
    mail_from: Option<String>,
    rcpt_to: Vec<String>,
    data: Vec<u8>,
    auth_seen: bool,
}

/// A running scripted server bound to an ephemeral local port.
struct TestServer {
    port: u16,
    cert_der: Vec<u8>,
    capture: Arc<Mutex<Capture>>,
}

impl TestServer {
    async fn start(script: Script) -> Self {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("generate cert");
        let cert_der = cert.der().to_vec();
        let server_config = build_server_config(cert.der().clone(), signing_key.serialize_der());
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let capture = Arc::new(Mutex::new(Capture::default()));
        let capture_task = capture.clone();

        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                let _ = handle_conn(tcp, script, acceptor, capture_task).await;
            }
        });

        Self {
            port,
            cert_der,
            capture,
        }
    }

    fn config(&self, security: ConnectionSecurity, timeout: Duration) -> SmtpConfig {
        let endpoint = ServerEndpoint::new(
            "127.0.0.1",
            std::num::NonZeroU16::new(self.port).expect("nonzero port"),
        )
        .expect("endpoint");
        let mut config = SmtpConfig::new(endpoint, security, "ada@example.org", "s3cr3t-pw");
        config.timeout = timeout;
        config.extra_root_certs_der = vec![self.cert_der.clone()];
        config
    }
}

fn build_server_config(cert: CertificateDer<'static>, key_der: Vec<u8>) -> ServerConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![cert],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
        )
        .expect("server certificate")
}

/// Signal returned by the command loop so the caller can upgrade to TLS.
enum Flow {
    Done,
    StartTls,
}

async fn handle_conn(
    tcp: TcpStream,
    script: Script,
    acceptor: TlsAcceptor,
    capture: Arc<Mutex<Capture>>,
) -> std::io::Result<()> {
    if script.stall {
        // Accept the connection but never proceed with the handshake; the
        // client must give up on its own deadline.
        tokio::time::sleep(Duration::from_secs(10)).await;
        return Ok(());
    }

    match script.security {
        ConnectionSecurity::Tls => {
            let mut tls = acceptor.accept(tcp).await?;
            let mut line = Line::new(&mut tls);
            line.write_line(GREETING).await?;
            command_loop(&mut line, &script, &capture, false).await?;
        }
        ConnectionSecurity::StartTls => {
            let mut tcp = tcp;
            {
                let mut line = Line::new(&mut tcp);
                line.write_line(GREETING).await?;
                if let Flow::Done = command_loop(&mut line, &script, &capture, true).await? {
                    return Ok(());
                }
            }
            // STARTTLS was requested and acknowledged; upgrade and continue.
            let mut tls = acceptor.accept(tcp).await?;
            let mut line = Line::new(&mut tls);
            command_loop(&mut line, &script, &capture, false).await?;
        }
    }
    Ok(())
}

async fn command_loop<S>(
    line: &mut Line<'_, S>,
    script: &Script,
    capture: &Arc<Mutex<Capture>>,
    offer_starttls: bool,
) -> std::io::Result<Flow>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let command = match line.read_line().await? {
            Some(command) => command,
            None => return Ok(Flow::Done),
        };
        let upper = command.to_ascii_uppercase();

        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            write_ehlo(line, offer_starttls).await?;
        } else if upper.starts_with("STARTTLS") {
            line.write_line("220 2.0.0 Ready to start TLS").await?;
            return Ok(Flow::StartTls);
        } else if upper.starts_with("AUTH") {
            handle_auth(line, &command, script, capture).await?;
        } else if upper.starts_with("MAIL FROM") {
            capture.lock().unwrap().mail_from = Some(extract_angle(&command));
            line.write_line("250 2.1.0 Sender OK").await?;
        } else if upper.starts_with("RCPT TO") {
            capture
                .lock()
                .unwrap()
                .rcpt_to
                .push(extract_angle(&command));
            line.write_line(&script.rcpt_reply).await?;
        } else if upper.starts_with("DATA") {
            line.write_line("354 End data with <CR><LF>.<CR><LF>")
                .await?;
            let body = read_data(line).await?;
            capture.lock().unwrap().data = body;
            line.write_line(&script.data_reply).await?;
        } else if upper.starts_with("QUIT") {
            line.write_line("221 2.0.0 Bye").await?;
            return Ok(Flow::Done);
        } else if upper.starts_with("RSET") || upper.starts_with("NOOP") {
            line.write_line("250 2.0.0 OK").await?;
        } else {
            line.write_line("500 5.5.2 Command unrecognized").await?;
        }
    }
}

async fn write_ehlo<S>(line: &mut Line<'_, S>, offer_starttls: bool) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut caps: Vec<String> = vec!["test.local".to_owned(), "AUTH PLAIN LOGIN".to_owned()];
    if offer_starttls {
        caps.push("STARTTLS".to_owned());
    }
    caps.push("SIZE 20480000".to_owned());
    caps.push("8BITMIME".to_owned());

    let last = caps.len() - 1;
    let mut out = String::new();
    for (index, capability) in caps.iter().enumerate() {
        let separator = if index == last { ' ' } else { '-' };
        out.push_str(&format!("250{separator}{capability}\r\n"));
    }
    line.write_raw(out.as_bytes()).await
}

async fn handle_auth<S>(
    line: &mut Line<'_, S>,
    command: &str,
    script: &Script,
    capture: &Arc<Mutex<Capture>>,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    capture.lock().unwrap().auth_seen = true;
    let upper = command.to_ascii_uppercase();

    if upper.starts_with("AUTH LOGIN") {
        line.write_line("334 VXNlcm5hbWU6").await?; // base64("Username:")
        let _ = line.read_line().await?;
        line.write_line("334 UGFzc3dvcmQ6").await?; // base64("Password:")
        let _ = line.read_line().await?;
    } else if upper.starts_with("AUTH PLAIN") {
        // mail-send sends the initial response inline; only prompt if absent.
        if command.split_whitespace().count() < 3 {
            line.write_line("334 ").await?;
            let _ = line.read_line().await?;
        }
    } else {
        line.write_line("504 5.5.4 Unrecognized authentication type")
            .await?;
        return Ok(());
    }

    if script.auth_succeeds {
        line.write_line("235 2.7.0 Authentication successful")
            .await?;
    } else {
        line.write_line("535 5.7.8 Authentication credentials invalid")
            .await?;
    }
    Ok(())
}

async fn read_data<S>(line: &mut Line<'_, S>) -> std::io::Result<Vec<u8>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut data = Vec::new();
    while let Some(entry) = line.read_line().await? {
        if entry == "." {
            break;
        }
        // Reverse the SMTP transparency dot-stuffing.
        let content = entry.strip_prefix('.').unwrap_or(&entry);
        data.extend_from_slice(content.as_bytes());
        data.extend_from_slice(b"\r\n");
    }
    Ok(data)
}

fn extract_angle(command: &str) -> String {
    match (command.find('<'), command.rfind('>')) {
        (Some(open), Some(close)) if open < close => command[open + 1..close].to_owned(),
        _ => String::new(),
    }
}

/// Minimal CRLF line reader/writer over a borrowed async stream.
struct Line<'a, S> {
    stream: &'a mut S,
    buf: Vec<u8>,
}

impl<'a, S> Line<'a, S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: &'a mut S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    async fn read_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = self.buf.drain(..=pos).collect();
                let text = String::from_utf8_lossy(&raw);
                return Ok(Some(text.trim_end_matches(['\r', '\n']).to_owned()));
            }
            let mut chunk = [0u8; 1024];
            let read = self.stream.read(&mut chunk).await?;
            if read == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                let text = String::from_utf8_lossy(&self.buf);
                let line = text.trim_end_matches(['\r', '\n']).to_owned();
                self.buf.clear();
                return Ok(Some(line));
            }
            self.buf.extend_from_slice(&chunk[..read]);
        }
    }

    async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stream.write_all(line.as_bytes()).await?;
        self.stream.write_all(b"\r\n").await?;
        self.stream.flush().await
    }

    async fn write_raw(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(data).await?;
        self.stream.flush().await
    }
}

// ---------------------------------------------------------------------------
// Message + driver helpers
// ---------------------------------------------------------------------------

fn address(name: Option<&str>, email: &str) -> Address {
    Address {
        display_name: name.map(str::to_owned),
        email: EmailAddress::parse(email).expect("valid address"),
    }
}

/// A rich message: To + Cc + two Bcc, Reply-To, HTML alternative, attachment.
fn sample_message() -> OutgoingMessage {
    OutgoingMessage {
        from: address(Some("Ada Lovelace"), "ada@example.org"),
        to: vec![address(Some("Bob"), "bob@example.org")],
        cc: vec![address(Some("Carol"), "carol@example.net")],
        bcc: vec![
            address(None, "dan@example.com"),
            address(None, "erin@example.org"),
        ],
        reply_to: Some(address(None, "replies@example.org")),
        subject: "Meeting notes".to_owned(),
        text_body: "Hello in plain text.".to_owned(),
        html_body: Some("<p>Hello in <b>HTML</b></p>".to_owned()),
        attachments: vec![OutgoingAttachment {
            filename: "report.pdf".to_owned(),
            mime_type: "application/pdf".to_owned(),
            bytes: PDF_BYTES.to_vec(),
        }],
        in_reply_to: None,
        references: vec![],
        disposition_notification_to: None,
    }
}

/// Starts a server, builds a provider, and submits [`sample_message`].
async fn submit(
    script: Script,
    timeout: Duration,
) -> (Result<SendResult, MailError>, Arc<Mutex<Capture>>) {
    let security = script.security;
    let server = TestServer::start(script).await;
    let config = server.config(security, timeout);
    let provider = SmtpProvider::new(config).expect("provider");
    let result = provider.send(&sample_message()).await;
    (result, server.capture.clone())
}

fn long_timeout() -> Duration {
    Duration::from_secs(10)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn successful_submission_over_tls() {
    let mut script = Script::tls();
    // The final reply echoes an address to exercise redaction.
    script.data_reply = "250 2.0.0 Ok: queued for <bob@example.org>".to_owned();

    let (result, capture) = submit(script, long_timeout()).await;
    let reply = result.expect("submission should succeed");

    // Server reply is captured and redacted (no full address survives).
    let server_reply = reply.server_reply.expect("server reply present");
    assert!(server_reply.starts_with("250"), "reply: {server_reply}");
    assert!(
        !server_reply.contains("bob@example.org"),
        "address must be redacted: {server_reply}"
    );

    let capture = capture.lock().unwrap();
    assert!(capture.auth_seen, "server should have authenticated");
    assert_eq!(capture.mail_from.as_deref(), Some("ada@example.org"));

    // Every To/Cc/Bcc recipient appears in the envelope (RCPT TO).
    for recipient in [
        "bob@example.org",
        "carol@example.net",
        "dan@example.com",
        "erin@example.org",
    ] {
        assert!(
            capture.rcpt_to.iter().any(|r| r == recipient),
            "missing RCPT TO for {recipient}: {:?}",
            capture.rcpt_to
        );
    }

    // Parse the captured DATA and assert MIME structure + headers.
    let parsed = MessageParser::default()
        .parse(&capture.data)
        .expect("message parses");

    // multipart/alternative wrapper is present, with both bodies.
    let raw = String::from_utf8_lossy(&capture.data);
    assert!(
        raw.contains("multipart/alternative"),
        "expected a multipart/alternative part"
    );
    assert!(
        parsed.text_body_count() >= 1 && parsed.html_body_count() >= 1,
        "both text and html alternatives expected"
    );
    assert!(
        parsed
            .body_text(0)
            .map(|body| body.contains("Hello in plain text"))
            .unwrap_or(false)
    );
    assert!(
        parsed
            .body_html(0)
            .map(|body| body.contains("<b>HTML</b>"))
            .unwrap_or(false)
    );

    // Attachment survives base64 round-trip with correct name and type.
    assert_eq!(parsed.attachment_count(), 1);
    let attachment = parsed.attachment(0).expect("attachment present");
    assert_eq!(attachment.attachment_name(), Some("report.pdf"));
    assert_eq!(attachment.contents(), PDF_BYTES);
    let content_type = attachment.content_type().expect("attachment content-type");
    assert_eq!(content_type.ctype(), "application");
    assert_eq!(content_type.subtype(), Some("pdf"));

    // Headers are correct.
    assert_eq!(parsed.subject(), Some("Meeting notes"));
    assert!(
        parsed
            .from()
            .and_then(|a| a.first())
            .and_then(|a| a.address())
            == Some("ada@example.org")
    );
    assert!(parsed.to().expect("to").contains("bob@example.org"));
    assert!(parsed.cc().expect("cc").contains("carol@example.net"));
    assert!(
        parsed
            .reply_to()
            .expect("reply-to")
            .contains("replies@example.org")
    );

    // A Message-ID and Date were generated.
    assert!(parsed.message_id().is_some());
    assert!(parsed.date().is_some());

    // Bcc must never be written as a header, even though the recipients were
    // delivered via RCPT TO above.
    assert!(parsed.bcc().is_none(), "Bcc must not appear as a header");
    assert!(
        parsed.header_raw("Bcc").is_none(),
        "raw Bcc header must be absent"
    );
    assert!(
        !raw.to_ascii_lowercase().contains("\r\nbcc:"),
        "no Bcc header line in the raw message"
    );
}

#[tokio::test]
async fn submission_over_starttls() {
    let (result, capture) = submit(Script::starttls(), long_timeout()).await;
    assert!(result.is_ok(), "STARTTLS submission failed: {result:?}");

    let capture = capture.lock().unwrap();
    assert!(capture.auth_seen);
    // Data was captured over the upgraded (TLS) channel.
    let parsed = MessageParser::default()
        .parse(&capture.data)
        .expect("message parses");
    assert_eq!(parsed.subject(), Some("Meeting notes"));
    assert!(capture.rcpt_to.iter().any(|r| r == "dan@example.com"));
}

#[tokio::test]
async fn wrong_credentials_map_to_authentication() {
    let mut script = Script::tls();
    script.auth_succeeds = false;

    let (result, _capture) = submit(script, long_timeout()).await;
    let error = result.expect_err("authentication should fail");
    assert!(
        matches!(error, MailError::Authentication),
        "expected Authentication, got {error:?}"
    );
}

#[tokio::test]
async fn transient_replies_are_temporary() {
    for code in [
        "450 4.2.0 Mailbox busy, try again",
        "452 4.3.1 Insufficient storage",
    ] {
        let mut script = Script::tls();
        script.rcpt_reply = code.to_owned();

        let (result, _capture) = submit(script, long_timeout()).await;
        let error = result.expect_err("transient reply should error");
        assert!(
            error.is_temporary(),
            "reply {code} should classify Temporary, got {error:?}"
        );
    }
}

#[tokio::test]
async fn permanent_reply_maps_to_protocol() {
    let mut script = Script::tls();
    script.rcpt_reply = "550 5.1.1 No such user <bob@example.org>".to_owned();

    let (result, _capture) = submit(script, long_timeout()).await;
    let error = result.expect_err("permanent reply should error");
    assert!(
        matches!(error, MailError::Protocol { .. }),
        "expected Protocol, got {error:?}"
    );
    assert_eq!(error.class(), axomail_core::ErrorClass::Permanent);
    if let MailError::Protocol { message } = error {
        assert!(
            !message.contains("bob@example.org"),
            "echoed address must be redacted: {message}"
        );
    }
}

#[tokio::test]
async fn server_stall_maps_to_timeout() {
    let mut script = Script::tls();
    script.stall = true;

    let (result, _capture) = submit(script, Duration::from_millis(300)).await;
    let error = result.expect_err("stalled server should time out");
    assert!(
        matches!(error, MailError::Timeout { .. }),
        "expected Timeout, got {error:?}"
    );
}

#[tokio::test]
async fn empty_recipient_list_fails_before_network_io() {
    let server = TestServer::start(Script::tls()).await;
    let config = server.config(ConnectionSecurity::Tls, long_timeout());
    let provider = SmtpProvider::new(config).expect("provider");

    let mut message = sample_message();
    message.to.clear();

    let error = provider
        .send(&message)
        .await
        .expect_err("empty recipient list should error");
    assert!(
        matches!(error, MailError::Configuration { .. }),
        "expected Configuration, got {error:?}"
    );

    // No dialogue happened: the provider returned before connecting.
    let capture = server.capture.lock().unwrap();
    assert!(capture.mail_from.is_none());
    assert!(capture.rcpt_to.is_empty());
    assert!(!capture.auth_seen);
}
