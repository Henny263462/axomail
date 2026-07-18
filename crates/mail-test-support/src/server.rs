//! A background scripted server bound to an ephemeral loopback port.
//!
//! [`ScriptedServer`] binds `127.0.0.1:0`, reports the assigned port, and
//! serves the same dialog to every accepted connection from a background
//! task. Dropping the handle aborts that task and, with it, every in-flight
//! connection task, so tests never leak work between cases.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio_rustls::TlsAcceptor;

use crate::error::TestServerError;
use crate::script::{Step, Transcript, lock, run_script};
use crate::tls::TestTlsIdentity;

/// Transport the server presents to clients.
#[derive(Clone)]
enum Transport {
    /// Unencrypted TCP.
    Plain,
    /// TLS terminated with the given acceptor.
    Tls(TlsAcceptor),
}

/// Shared list of per-connection transcripts, newest last.
type TranscriptLog = Arc<Mutex<Vec<Arc<Mutex<Transcript>>>>>;

/// A running scripted test server.
///
/// The server lives until the value is dropped. It accepts connections in a
/// loop and replays the same script for each, which supports reconnect and
/// restart scenarios as well as single-shot dialogs.
pub struct ScriptedServer {
    addr: SocketAddr,
    transcripts: TranscriptLog,
    handle: Option<JoinHandle<()>>,
}

impl ScriptedServer {
    /// Starts a plain-TCP server serving `steps` to each connection.
    ///
    /// # Errors
    ///
    /// Returns [`TestServerError::Io`] if the loopback socket cannot be
    /// bound.
    pub async fn spawn_plain(steps: Vec<Step>) -> Result<Self, TestServerError> {
        Self::spawn(steps, Transport::Plain).await
    }

    /// Starts a TLS server presenting `identity` and serving `steps`.
    ///
    /// # Errors
    ///
    /// Returns [`TestServerError::Io`] if the loopback socket cannot be
    /// bound.
    pub async fn spawn_tls(
        steps: Vec<Step>,
        identity: &TestTlsIdentity,
    ) -> Result<Self, TestServerError> {
        Self::spawn(steps, Transport::Tls(identity.acceptor())).await
    }

    async fn spawn(steps: Vec<Step>, transport: Transport) -> Result<Self, TestServerError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let transcripts: TranscriptLog = Arc::new(Mutex::new(Vec::new()));

        let handle = tokio::spawn(accept_loop(
            listener,
            Arc::new(steps),
            transport,
            Arc::clone(&transcripts),
        ));

        Ok(Self {
            addr,
            transcripts,
            handle: Some(handle),
        })
    }

    /// Socket address the server is listening on.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// TCP port the server is listening on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Snapshot of every connection's transcript so far, oldest first.
    #[must_use]
    pub fn transcripts(&self) -> Vec<Transcript> {
        lock(&self.transcripts)
            .iter()
            .map(|entry| lock(entry).clone())
            .collect()
    }

    /// Number of connections accepted so far.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        lock(&self.transcripts).len()
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Accepts connections until aborted, running the script for each.
///
/// Connection tasks live in a [`JoinSet`]; when this loop is aborted on
/// drop the set is dropped too, which aborts any still-running connection.
async fn accept_loop(
    listener: TcpListener,
    steps: Arc<Vec<Step>>,
    transport: Transport,
    transcripts: TranscriptLog,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    tracing::debug!(%peer, "test server accepted connection");
                    let transcript = Arc::new(Mutex::new(Transcript::default()));
                    lock(&transcripts).push(Arc::clone(&transcript));
                    let steps = Arc::clone(&steps);
                    let transport = transport.clone();
                    connections.spawn(handle_connection(stream, steps, transport, transcript));
                }
                Err(err) => {
                    tracing::warn!(error = %err, "test server accept failed; stopping");
                    break;
                }
            },
            Some(_joined) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

/// Runs the script for one connection, recording any failure.
async fn handle_connection(
    stream: TcpStream,
    steps: Arc<Vec<Step>>,
    transport: Transport,
    transcript: Arc<Mutex<Transcript>>,
) {
    let result = match transport {
        Transport::Plain => run_script(stream, &steps, &transcript).await,
        Transport::Tls(acceptor) => match acceptor.accept(stream).await {
            Ok(tls_stream) => run_script(tls_stream, &steps, &transcript).await,
            Err(err) => Err(TestServerError::Tls(err.to_string())),
        },
    };

    if let Err(err) = result {
        tracing::debug!(error = %err, "test server dialog ended with error");
        let mut recorded = lock(&transcript);
        if recorded.error.is_none() {
            recorded.error = Some(err.to_string());
        }
    }
}
