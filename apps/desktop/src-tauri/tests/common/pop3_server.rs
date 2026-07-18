//! A minimal dynamic POP3-over-TLS server for the composition sync tests.
//!
//! Unlike [`axomail_test_support::ScriptedServer`], which replays a fixed
//! step list, this server answers each command from a fixed message set on
//! the fly, so a client may reconnect and issue a different command sequence
//! on the second sync (fewer `TOP`s once messages are known). It speaks only
//! the subset the adapter uses over implicit TLS: greeting, `CAPA` (answered
//! `-ERR`, i.e. capabilities unknown), `USER`/`PASS`, `STAT`, `LIST`, `UIDL`,
//! `TOP n 0`, `RETR n`, `DELE n`, and `QUIT`.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use axomail_test_support::TestTlsIdentity;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// One message the server can serve, identified by its `UIDL`.
#[derive(Clone)]
pub struct Pop3Message {
    pub uidl: String,
    /// Complete RFC 5322 message with CRLF line endings.
    pub full: String,
}

impl Pop3Message {
    /// Builds a small message with a subject, sender, and a fixed date.
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

    fn header_block(&self) -> String {
        match self.full.find("\r\n\r\n") {
            Some(idx) => self.full[..idx + 4].to_owned(),
            None => self.full.clone(),
        }
    }
}

/// A running POP3 test server; its accept loop is aborted on drop.
pub struct Pop3TestServer {
    pub port: u16,
    pub root_der: Vec<u8>,
    commands: Arc<Mutex<Vec<String>>>,
    handle: JoinHandle<()>,
}

impl Pop3TestServer {
    /// Every command line received across all connections, in order.
    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().expect("commands lock").clone()
    }

    /// Whether any connection received a command starting with `prefix`.
    pub fn saw_command(&self, prefix: &str) -> bool {
        self.commands()
            .iter()
            .any(|command| command.starts_with(prefix))
    }
}

impl Drop for Pop3TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Spawns a POP3-over-TLS server serving `messages`, accepting `password`.
pub async fn spawn(messages: Vec<Pop3Message>, password: &str) -> Pop3TestServer {
    let identity = TestTlsIdentity::generate().expect("tls identity");
    let root_der = identity.root_der().to_vec();
    let acceptor = identity.acceptor();
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let commands = Arc::new(Mutex::new(Vec::new()));

    let messages = Arc::new(messages);
    let password = password.to_owned();
    let commands_task = Arc::clone(&commands);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            let messages = Arc::clone(&messages);
            let password = password.clone();
            let commands = Arc::clone(&commands_task);
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let _ = serve(tls, &messages, &password, &commands).await;
                }
            });
        }
    });

    Pop3TestServer {
        port,
        root_der,
        commands,
        handle,
    }
}

/// Serves one connection until `QUIT` or the client disconnects.
async fn serve<S>(
    stream: S,
    messages: &[Pop3Message],
    password: &str,
    commands: &Mutex<Vec<String>>,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read_half, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    write.write_all(b"+OK axomail test POP3 ready\r\n").await?;

    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let command = line.trim_end().to_owned();
        commands
            .lock()
            .expect("commands lock")
            .push(command.clone());
        if !dispatch(&mut write, &command, messages, password).await? {
            break;
        }
    }
    Ok(())
}

/// Handles one command; returns `false` once the session should close.
async fn dispatch<W>(
    write: &mut W,
    command: &str,
    messages: &[Pop3Message],
    password: &str,
) -> std::io::Result<bool>
where
    W: AsyncWrite + Unpin,
{
    let upper = command.to_ascii_uppercase();
    if upper == "CAPA" {
        // Report capabilities as unknown; the adapter then assumes TOP.
        write.write_all(b"-ERR unsupported\r\n").await?;
    } else if upper.starts_with("USER") {
        write.write_all(b"+OK user accepted\r\n").await?;
    } else if upper.starts_with("PASS") {
        let ok = command.strip_prefix("PASS ").is_some_and(|p| p == password);
        write
            .write_all(if ok {
                b"+OK mailbox ready\r\n".as_slice()
            } else {
                b"-ERR invalid password\r\n".as_slice()
            })
            .await?;
    } else if upper == "STAT" {
        let total: usize = messages.iter().map(|m| m.full.len()).sum();
        write
            .write_all(format!("+OK {} {total}\r\n", messages.len()).as_bytes())
            .await?;
    } else if upper == "LIST" {
        let lines: Vec<String> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{} {}", i + 1, m.full.len()))
            .collect();
        write_multiline(write, "+OK scan listing", &lines).await?;
    } else if upper == "UIDL" {
        let lines: Vec<String> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{} {}", i + 1, m.uidl))
            .collect();
        write_multiline(write, "+OK unique-id listing", &lines).await?;
    } else if let Some(rest) = upper.strip_prefix("TOP ") {
        let number = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        top(write, messages, number).await?;
    } else if let Some(rest) = upper.strip_prefix("RETR ") {
        retr(write, messages, rest.trim().parse().ok()).await?;
    } else if upper.starts_with("DELE") {
        write.write_all(b"+OK deleted\r\n").await?;
    } else if upper == "QUIT" {
        write.write_all(b"+OK bye\r\n").await?;
        return Ok(false);
    } else {
        write.write_all(b"-ERR unknown command\r\n").await?;
    }
    Ok(true)
}

/// Answers `TOP number 0` with the message's header block.
async fn top<W>(
    write: &mut W,
    messages: &[Pop3Message],
    number: Option<usize>,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match number.and_then(|n| messages.get(n.wrapping_sub(1))) {
        Some(message) => {
            let block = message.header_block();
            let lines: Vec<&str> = block.split("\r\n").filter(|l| !l.is_empty()).collect();
            let owned: Vec<String> = lines.into_iter().map(str::to_owned).collect();
            write_multiline(write, "+OK top follows", &owned).await
        }
        None => write.write_all(b"-ERR no such message\r\n").await,
    }
}

/// Answers `RETR number` with the full dot-stuffed message.
async fn retr<W>(
    write: &mut W,
    messages: &[Pop3Message],
    number: Option<usize>,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match number.and_then(|n| messages.get(n.wrapping_sub(1))) {
        Some(message) => {
            write
                .write_all(format!("+OK {} octets\r\n", message.full.len()).as_bytes())
                .await?;
            let lines: Vec<&str> = message
                .full
                .split("\r\n")
                .collect::<Vec<_>>()
                .into_iter()
                .filter(|l| !l.is_empty())
                .collect();
            let owned: Vec<String> = lines.into_iter().map(str::to_owned).collect();
            write_body(write, &owned).await
        }
        None => write.write_all(b"-ERR no such message\r\n").await,
    }
}

/// Writes an intro line plus dot-terminated body lines.
async fn write_multiline<W>(write: &mut W, intro: &str, lines: &[String]) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    write.write_all(format!("{intro}\r\n").as_bytes()).await?;
    write_body(write, lines).await
}

/// Writes dot-stuffed body lines followed by the `.` terminator.
async fn write_body<W>(write: &mut W, lines: &[String]) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut out = String::new();
    for line in lines {
        if line.starts_with('.') {
            out.push('.');
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str(".\r\n");
    write.write_all(out.as_bytes()).await
}
