//! Integration tests for account auto-configuration: the layered lookup driven
//! against a local stub HTTP server, plus the XML parser and the
//! common-provider guess fallback.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use axomail_core::{ConnectionSecurity, IncomingProtocol};
use axomail_desktop_lib::autoconfig::{
    AutoconfigSource, FetchPolicy, discover_from_sources, guess_config, parse_autoconfig,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A representative Mozilla ISPDB response (Gmail), trimmed to the elements the
/// parser consumes.
const ISPDB_GMAIL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="googlemail.com">
    <domain>gmail.com</domain>
    <displayName>Google Mail</displayName>
    <displayShortName>GMail</displayShortName>
    <incomingServer type="imap">
      <hostname>imap.gmail.com</hostname>
      <port>993</port>
      <socketType>SSL</socketType>
      <username>%EMAILADDRESS%</username>
      <authentication>OAuth2</authentication>
      <authentication>password-cleartext</authentication>
    </incomingServer>
    <incomingServer type="pop3">
      <hostname>pop.gmail.com</hostname>
      <port>995</port>
      <socketType>SSL</socketType>
      <username>%EMAILADDRESS%</username>
      <authentication>password-cleartext</authentication>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.gmail.com</hostname>
      <port>465</port>
      <socketType>SSL</socketType>
      <username>%EMAILADDRESS%</username>
      <authentication>OAuth2</authentication>
      <authentication>password-cleartext</authentication>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

/// Spins up a one-connection-per-response local HTTP server. Each accepted
/// connection is answered with the next `(status, body)` in order; the raw
/// request bytes are captured so a test can assert on what was (not) sent.
async fn stub(responses: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&captured);
    tokio::spawn(async move {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).into_owned());
            let reason = if status == 200 { "OK" } else { "Not Found" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: text/xml\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });
    (url, captured)
}

/// A plain reqwest client for the local HTTP stub. reqwest's
/// `rustls-no-provider` build requires a crypto provider to be installed before
/// a `Client` is constructed (ADR-0009), even for plaintext requests.
fn client() -> reqwest::Client {
    axomail_desktop_lib::oauth::install_crypto_provider();
    reqwest::Client::builder().build().unwrap()
}

#[test]
fn parses_representative_ispdb_response() {
    let config = parse_autoconfig(ISPDB_GMAIL, "klaus@gmail.com").expect("parses");
    assert_eq!(config.display_name.as_deref(), Some("Google Mail"));
    assert_eq!(config.incoming.protocol, IncomingProtocol::Imap);
    assert_eq!(config.incoming.host, "imap.gmail.com");
    assert_eq!(config.incoming.port, 993);
    assert_eq!(config.incoming.security, ConnectionSecurity::Tls);
    assert_eq!(config.incoming.username, "klaus@gmail.com");
    assert!(config.incoming.oauth, "Gmail advertises OAuth2 first");
    assert_eq!(config.outgoing.host, "smtp.gmail.com");
    assert_eq!(config.outgoing.port, 465);
}

#[tokio::test]
async fn found_via_stub_server() {
    let (url, _captured) = stub(vec![(200, ISPDB_GMAIL)]).await;
    let sources = vec![(AutoconfigSource::Ispdb, format!("{url}/v1.1/gmail.com"))];
    let outcome = discover_from_sources(
        &client(),
        &sources,
        "klaus@gmail.com",
        FetchPolicy::Unrestricted,
    )
    .await
    .expect("a 200 with valid XML is found");
    assert_eq!(outcome.source, AutoconfigSource::Ispdb);
    assert_eq!(outcome.config.incoming.host, "imap.gmail.com");
}

#[tokio::test]
async fn not_found_when_server_404s() {
    let (url, _captured) = stub(vec![(404, "")]).await;
    let sources = vec![(
        AutoconfigSource::Ispdb,
        format!("{url}/v1.1/nobody.example"),
    )];
    let outcome = discover_from_sources(
        &client(),
        &sources,
        "a@nobody.example",
        FetchPolicy::Unrestricted,
    )
    .await;
    assert!(outcome.is_none(), "a 404 yields no configuration");
}

#[tokio::test]
async fn malformed_body_yields_none() {
    let (url, _captured) = stub(vec![(200, "<this is not autoconfig>")]).await;
    let sources = vec![(AutoconfigSource::Autoconfig, format!("{url}/mail/config"))];
    let outcome = discover_from_sources(
        &client(),
        &sources,
        "a@x.example",
        FetchPolicy::Unrestricted,
    )
    .await;
    assert!(outcome.is_none(), "unparseable XML yields no configuration");
}

#[tokio::test]
async fn falls_through_to_the_next_source() {
    // First source returns garbage; the second returns a valid document.
    let (url, _captured) = stub(vec![(200, "garbage"), (200, ISPDB_GMAIL)]).await;
    let sources = vec![
        (AutoconfigSource::Autoconfig, format!("{url}/mail/config")),
        (AutoconfigSource::Ispdb, format!("{url}/v1.1/gmail.com")),
    ];
    let outcome = discover_from_sources(
        &client(),
        &sources,
        "klaus@gmail.com",
        FetchPolicy::Unrestricted,
    )
    .await
    .expect("the second source succeeds");
    assert_eq!(outcome.source, AutoconfigSource::Ispdb);
}

#[tokio::test]
async fn no_credentials_are_sent() {
    let (url, captured) = stub(vec![(200, ISPDB_GMAIL)]).await;
    // A distinctive local part and password so leakage would be obvious.
    let sources = vec![(AutoconfigSource::Ispdb, format!("{url}/v1.1/gmail.com"))];
    let _ = discover_from_sources(
        &client(),
        &sources,
        "secretlocal@gmail.com",
        FetchPolicy::Unrestricted,
    )
    .await;

    let requests = captured.lock().unwrap();
    let request = requests.first().expect("the stub received one request");
    assert!(request.starts_with("GET "), "lookup is a GET: {request}");
    assert!(
        !request.contains("secretlocal"),
        "the local part must never appear in the request: {request}"
    );
    assert!(
        !request.to_ascii_lowercase().contains("password"),
        "no password material may be sent: {request}"
    );
    // Only the domain is in the path.
    assert!(
        request.contains("gmail.com"),
        "the domain drives the lookup"
    );
}

#[test]
fn guess_fallback_covers_common_providers() {
    let gmail = guess_config("gmail.com", "klaus@gmail.com").expect("gmail is a known provider");
    assert_eq!(gmail.incoming.host, "imap.gmail.com");
    assert_eq!(gmail.outgoing.host, "smtp.gmail.com");
    assert_eq!(gmail.incoming.username, "klaus@gmail.com");

    // An unknown domain has no guess — setup falls back to manual entry.
    assert!(guess_config("weird-corp.example", "a@weird-corp.example").is_none());
}
