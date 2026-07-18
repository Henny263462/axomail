//! The [`SmtpProvider`] adapter: connect, authenticate, submit one message.

use axomail_core::{
    ConnectionSecurity, MailError, OutgoingMailProvider, OutgoingMessage, SendResult,
};
use mail_send::smtp::AssertReply;
use mail_send::smtp::message::Parameters;
use mail_send::{Credentials, SmtpClient, SmtpClientBuilder};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::config::SmtpConfig;
use crate::error_map::{map_reply, map_send_error};
use crate::mime::{HELO_HOST, build_message_bytes, collect_recipients};
use crate::redact::redact_reply_line;
use crate::tls::build_tls_connector;

/// Submits mail over an authenticated SMTP session.
///
/// Each [`SmtpProvider::send`] call opens a fresh connection (implicit TLS or
/// STARTTLS per [`SmtpConfig::security`]), authenticates, submits one message,
/// and closes the session. The strict [`TlsConnector`] is built once at
/// construction and shared across sends.
pub struct SmtpProvider {
    config: SmtpConfig,
    tls_connector: TlsConnector,
}

impl SmtpProvider {
    /// Builds a provider, constructing the strict TLS connector up front.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Tls`] if [`SmtpConfig::extra_root_certs_der`]
    /// contains a certificate that cannot be parsed.
    pub fn new(config: SmtpConfig) -> Result<Self, MailError> {
        let tls_connector = build_tls_connector(&config.extra_root_certs_der)?;
        Ok(Self {
            config,
            tls_connector,
        })
    }

    /// Assembles a mail-send client builder wired to the strict connector.
    ///
    /// Every field is set explicitly; the public `tls_connector` field carries
    /// our own connector, so mail-send's platform verifier and its
    /// `allow_invalid_certs` path are never involved.
    fn client_builder(&self, host: &str, port: u16) -> SmtpClientBuilder<String> {
        SmtpClientBuilder {
            addr: format!("{host}:{port}"),
            timeout: self.config.timeout,
            tls_connector: self.tls_connector.clone(),
            tls_hostname: host.to_owned(),
            tls_implicit: matches!(self.config.security, ConnectionSecurity::Tls),
            credentials: Some(Credentials::new(
                self.config.username.clone(),
                self.config.password.clone(),
            )),
            is_lmtp: false,
            say_ehlo: true,
            local_host: HELO_HOST.to_owned(),
            local_ip: None,
        }
    }

    /// Runs the envelope + DATA exchange, returning the redacted final reply.
    ///
    /// The whole exchange is bounded by one [`tokio::time::timeout`] so a stall
    /// at any step surfaces as [`MailError::Timeout`]. The final reply is
    /// captured (mail-send's high-level `send` discards it) so it can be
    /// returned in the [`SendResult`].
    async fn submit(
        &self,
        client: &mut SmtpClient<TlsStream<TcpStream>>,
        sender: &str,
        recipients: &[String],
        body: &[u8],
    ) -> Result<String, MailError> {
        let deadline = self.config.timeout;
        let exchange = tokio::time::timeout(deadline, async {
            client.mail_from(sender, &Parameters::default()).await?;
            for recipient in recipients {
                client
                    .rcpt_to(recipient.as_str(), &Parameters::default())
                    .await?;
            }
            client.cmd(b"DATA\r\n").await?.assert_code(354)?;
            client.write_message(body).await?;
            client.read().await
        })
        .await;

        let reply = match exchange {
            Err(_) => return Err(MailError::Timeout { after: deadline }),
            Ok(Err(err)) => return Err(map_send_error(err, deadline)),
            Ok(Ok(reply)) => reply,
        };

        if !reply.is_positive_completion() {
            return Err(map_reply(reply.code, &reply.message));
        }

        let line = redact_reply_line(reply.code, &reply.message);
        // Best-effort graceful close; the message is already accepted.
        let _ = client.cmd(b"QUIT\r\n").await;
        Ok(line)
    }
}

impl OutgoingMailProvider for SmtpProvider {
    /// Submits one message. Validates recipients before any network I/O, then
    /// connects, authenticates, and transmits.
    async fn send(&self, message: &OutgoingMessage) -> Result<SendResult, MailError> {
        if message.to.is_empty() {
            return Err(MailError::Configuration {
                message: "outgoing message has no primary (To) recipient".to_owned(),
            });
        }

        let body = build_message_bytes(message)?;
        let recipients = collect_recipients(message);
        let sender = message.from.email.as_str();

        let host = self.config.endpoint.host();
        let port = self.config.endpoint.port().get();
        // Only non-sensitive metadata is logged: never addresses or credentials.
        tracing::debug!(
            host,
            port,
            recipient_count = recipients.len(),
            body_bytes = body.len(),
            "submitting message via SMTP"
        );
        let builder = self.client_builder(host, port);

        // `connect` performs the TLS/STARTTLS handshake, EHLO, and AUTH, all
        // bounded internally by the configured timeout.
        let mut client = builder
            .connect()
            .await
            .map_err(|err| map_send_error(err, self.config.timeout))?;

        let server_reply = self.submit(&mut client, sender, &recipients, &body).await?;
        // The reply is already redacted; safe to log for diagnostics.
        tracing::debug!(reply = server_reply.as_str(), "SMTP submission accepted");
        Ok(SendResult {
            server_reply: Some(server_reply),
        })
    }
}
