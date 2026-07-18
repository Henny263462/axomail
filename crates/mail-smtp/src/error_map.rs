//! Translation of [`mail_send::Error`] into [`axomail_core::MailError`].
//!
//! Classification follows ADR-0003: TLS failures are permanent and never
//! downgraded, authentication rejections carry no payload, 4xx replies are
//! temporary, and 5xx replies are permanent protocol errors. Every reply string
//! is redacted before it reaches a message field.

use std::time::Duration;

use axomail_core::MailError;
use mail_send::Error as SendError;

use crate::redact::redact;

/// Maps a mail-send error to the core error hierarchy.
///
/// `timeout` is the configured deadline, reported verbatim on
/// [`MailError::Timeout`].
pub(crate) fn map_send_error(error: SendError, timeout: Duration) -> MailError {
    match error {
        SendError::Io(err) => MailError::Network {
            message: redact(&err.to_string()),
        },
        SendError::Tls(err) => MailError::Tls {
            message: redact(&err.to_string()),
        },
        SendError::Base64(_) => MailError::Protocol {
            message: "server sent an undecodable base64 response".to_owned(),
        },
        // A rejected credential (535) or an exhausted mechanism list both mean
        // the presented credentials were not accepted.
        SendError::Auth(_) | SendError::AuthenticationFailed(_) => MailError::Authentication,
        SendError::UnexpectedReply(reply) => map_reply(reply.code, &reply.message),
        SendError::UnparseableReply => MailError::Protocol {
            message: "server sent an unparseable reply".to_owned(),
        },
        SendError::InvalidTLSName => MailError::Configuration {
            message: "server host is not a valid TLS certificate name".to_owned(),
        },
        SendError::MissingCredentials => MailError::Configuration {
            message: "server requires authentication but no credentials were supplied".to_owned(),
        },
        SendError::MissingMailFrom => MailError::Configuration {
            message: "outgoing message has no sender address".to_owned(),
        },
        SendError::MissingRcptTo => MailError::Configuration {
            message: "outgoing message has no recipients".to_owned(),
        },
        SendError::UnsupportedAuthMechanism => MailError::UnsupportedCapability {
            capability: "a mutually supported SMTP AUTH mechanism".to_owned(),
        },
        // The server did not advertise STARTTLS on a STARTTLS connection;
        // mail-send refuses to continue in cleartext and so do we.
        SendError::MissingStartTls => MailError::UnsupportedCapability {
            capability: "STARTTLS".to_owned(),
        },
        SendError::Timeout => MailError::Timeout { after: timeout },
    }
}

/// Classifies a numeric SMTP reply that terminated an operation.
///
/// 4xx replies are temporary (throttle-shaped codes become
/// [`MailError::RateLimited`], the rest [`MailError::Network`]); everything else
/// — chiefly 5xx — is a permanent [`MailError::Protocol`] error.
pub(crate) fn map_reply(code: u16, message: &str) -> MailError {
    let detail = redact(message);
    match code {
        // Overload / "try again later" shaped codes.
        421 | 451 => MailError::RateLimited { retry_after: None },
        400..=499 => MailError::Network {
            message: format!("server returned transient error {code}: {detail}"),
        },
        _ => MailError::Protocol {
            message: format!("server returned error {code}: {detail}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axomail_core::ErrorClass;

    #[test]
    fn transient_codes_are_temporary() {
        for code in [421, 450, 451, 452] {
            assert_eq!(
                map_reply(code, "busy").class(),
                ErrorClass::Temporary,
                "code {code} should be temporary"
            );
        }
    }

    #[test]
    fn permanent_codes_are_protocol() {
        let err = map_reply(550, "mailbox <ada@example.org> unavailable");
        assert_eq!(err.class(), ErrorClass::Permanent);
        assert!(matches!(err, MailError::Protocol { .. }));
        // The echoed address must not survive into the message.
        if let MailError::Protocol { message } = err {
            assert!(!message.contains("ada@example.org"));
        }
    }
}
