//! Integration tests for the local protocol test servers.
//!
//! These act as protocol clients against [`ScriptedServer`], covering plain
//! and TLS transports, timing behaviour, transcript capture, and the POP3
//! convenience builder.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use axomail_test_support::{ScriptedServer, Step, TestTlsIdentity, fixtures, pop3};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Reads one line and returns it with the trailing line ending removed.
async fn expect_line<R>(reader: &mut R) -> String
where
    R: AsyncBufReadExt + Unpin,
{
    let mut buf = Vec::new();
    let read = reader.read_until(b'\n', &mut buf).await.expect("read line");
    assert!(read > 0, "connection closed unexpectedly");
    trim_line_ending(&buf)
}

/// Reads a POP3 multi-line block, un-stuffing dots, until the `.` terminator.
async fn read_until_dot<R>(reader: &mut R) -> Vec<String>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut lines = Vec::new();
    loop {
        let line = expect_line(reader).await;
        if line == "." {
            break;
        }
        lines.push(line.strip_prefix('.').map(str::to_owned).unwrap_or(line));
    }
    lines
}

/// Trims a trailing `\n` and any preceding `\r`.
fn trim_line_ending(raw: &[u8]) -> String {
    let mut end = raw.len();
    if end > 0 && raw[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && raw[end - 1] == b'\r' {
            end -= 1;
        }
    }
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// Reconstructs a message body from POP3 data lines using CRLF endings.
fn rejoin(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out
}

#[tokio::test]
async fn plain_tcp_round_trip_and_transcript_capture() {
    let script = vec![
        Step::send_line("* OK ready"),
        Step::expect_containing("PING"),
        Step::send_line("* PONG"),
    ];
    let server = ScriptedServer::spawn_plain(script).await.unwrap();

    let stream = TcpStream::connect(server.addr()).await.unwrap();
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);

    assert_eq!(expect_line(&mut reader).await, "* OK ready");
    // A bare LF terminator exercises the engine's CRLF tolerance.
    write_half.write_all(b"PING please\n").await.unwrap();
    assert_eq!(expect_line(&mut reader).await, "* PONG");

    // Drive the connection to completion so the transcript is final.
    drop(write_half);
    drop(reader);

    let transcripts = server.transcripts();
    assert_eq!(transcripts.len(), 1);
    let transcript = &transcripts[0];
    assert!(transcript.received_text().contains("PING please"));
    assert!(transcript.sent_text().contains("* PONG"));
    assert_eq!(transcript.error, None);
    // Steps 1 (send) and 2 (expect) are provably done once the reply that
    // step 3 sends has been read; step 3's own counter bump races with this
    // check, so assert only the race-free lower bound.
    assert!(transcript.steps_completed >= 2);
}

#[tokio::test]
async fn tls_round_trip_with_generated_identity() {
    let identity = TestTlsIdentity::generate().unwrap();
    let script = vec![
        Step::send_line("* OK tls ready"),
        Step::expect_containing("HELLO"),
        Step::send_line("* WELCOME"),
    ];
    let server = ScriptedServer::spawn_tls(script, &identity).await.unwrap();

    // The client trusts only the server's self-signed certificate via the
    // identity's root DER (used here through the ready-made connector).
    let connector = TlsConnector::from(std::sync::Arc::new(identity.client_config().unwrap()));
    let tcp = TcpStream::connect(server.addr()).await.unwrap();
    let domain = ServerName::try_from("localhost").unwrap();
    let tls = connector.connect(domain, tcp).await.expect("tls handshake");

    let (read_half, mut write_half) = tokio::io::split(tls);
    let mut reader = BufReader::new(read_half);

    assert_eq!(expect_line(&mut reader).await, "* OK tls ready");
    write_half.write_all(b"HELLO server\r\n").await.unwrap();
    assert_eq!(expect_line(&mut reader).await, "* WELCOME");

    drop(write_half);
    drop(reader);

    let transcripts = server.transcripts();
    assert_eq!(transcripts.len(), 1);
    assert!(transcripts[0].received_text().contains("HELLO server"));
    assert_eq!(transcripts[0].error, None);
}

#[tokio::test]
async fn tls_client_without_the_root_is_rejected() {
    let identity = TestTlsIdentity::generate().unwrap();
    let server = ScriptedServer::spawn_tls(vec![Step::send_line("* OK")], &identity)
        .await
        .unwrap();

    // A default root store does not trust the self-signed identity, so the
    // handshake must fail rather than silently succeed.
    let roots = rustls::RootCertStore::empty();
    let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = TlsConnector::from(std::sync::Arc::new(config));

    let tcp = TcpStream::connect(server.addr()).await.unwrap();
    let domain = ServerName::try_from("localhost").unwrap();
    let result = connector.connect(domain, tcp).await;
    assert!(result.is_err(), "handshake should fail without the root");
}

#[tokio::test]
async fn stall_delays_the_next_line() {
    let stall = Duration::from_millis(200);
    let script = vec![
        Step::send_line("first"),
        Step::StallMs(stall.as_millis() as u64),
        Step::send_line("second"),
    ];
    let server = ScriptedServer::spawn_plain(script).await.unwrap();

    let stream = TcpStream::connect(server.addr()).await.unwrap();
    let (read_half, _write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);

    assert_eq!(expect_line(&mut reader).await, "first");
    let started = Instant::now();
    assert_eq!(expect_line(&mut reader).await, "second");
    let elapsed = started.elapsed();

    // Allow scheduling slack but require most of the stall to have elapsed.
    assert!(
        elapsed >= Duration::from_millis(150),
        "second line arrived after only {elapsed:?}"
    );
}

#[tokio::test]
async fn pop3_builder_dialog_is_walkable() {
    let messages = [fixtures::PLAIN, fixtures::ALTERNATIVE];
    let script = pop3::download_session(&messages);
    let server = ScriptedServer::spawn_plain(script).await.unwrap();

    let stream = TcpStream::connect(server.addr()).await.unwrap();
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);

    // Greeting.
    assert!(expect_line(&mut reader).await.starts_with("+OK"));

    // Authentication.
    send(&mut write_half, "USER alice").await;
    assert!(expect_line(&mut reader).await.starts_with("+OK"));
    send(&mut write_half, "PASS secret").await;
    assert!(expect_line(&mut reader).await.starts_with("+OK"));

    // STAT reports the count and total size.
    send(&mut write_half, "STAT").await;
    let stat = expect_line(&mut reader).await;
    let total: usize = messages.iter().map(|m| m.len()).sum();
    assert_eq!(stat, format!("+OK {} {}", messages.len(), total));

    // LIST and UIDL each yield one entry per message.
    send(&mut write_half, "LIST").await;
    assert!(expect_line(&mut reader).await.starts_with("+OK"));
    assert_eq!(read_until_dot(&mut reader).await.len(), messages.len());

    send(&mut write_half, "UIDL").await;
    assert!(expect_line(&mut reader).await.starts_with("+OK"));
    assert_eq!(read_until_dot(&mut reader).await.len(), messages.len());

    // RETR each message and confirm the body round-trips byte for byte.
    for (index, original) in messages.iter().enumerate() {
        send(&mut write_half, &format!("RETR {}", index + 1)).await;
        assert!(expect_line(&mut reader).await.starts_with("+OK"));
        let body = rejoin(&read_until_dot(&mut reader).await);
        assert_eq!(&body, original, "message {} mismatch", index + 1);
    }

    send(&mut write_half, "QUIT").await;
    assert!(expect_line(&mut reader).await.starts_with("+OK"));

    assert_eq!(server.connection_count(), 1);
    assert_eq!(server.transcripts()[0].error, None);
}

#[tokio::test]
async fn serves_many_connections_without_leaking() {
    let script = vec![Step::send_line("hi"), Step::expect_containing("bye")];
    let server = ScriptedServer::spawn_plain(script).await.unwrap();
    let addr = server.addr();

    let clients = (0..5).map(|_| async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        let (read_half, mut write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        assert_eq!(expect_line(&mut reader).await, "hi");
        send(&mut write_half, "bye").await;
    });
    futures::future::join_all(clients).await;

    assert_eq!(server.connection_count(), 5);
}

#[tokio::test]
async fn dropping_the_server_closes_the_port() {
    let server = ScriptedServer::spawn_plain(vec![Step::send_line("hi")])
        .await
        .unwrap();
    let addr = server.addr();
    // Confirm it is live before shutdown.
    TcpStream::connect(addr).await.unwrap();
    drop(server);

    // After the handle drops, the accept task is aborted and the port frees.
    let mut refused = false;
    for _ in 0..100 {
        match TcpStream::connect(addr).await {
            Ok(_) => tokio::task::yield_now().await,
            Err(_) => {
                refused = true;
                break;
            }
        }
    }
    assert!(refused, "port stayed open after the server was dropped");
}

/// Writes a CRLF-terminated command to the server.
async fn send<W>(writer: &mut W, command: &str)
where
    W: AsyncWrite + Unpin,
{
    let mut bytes = command.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    writer.write_all(&bytes).await.unwrap();
    writer.flush().await.unwrap();
}
