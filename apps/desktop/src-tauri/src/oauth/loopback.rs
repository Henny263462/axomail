//! RFC 8252 loopback redirect receiver.
//!
//! A one-shot HTTP/1.1 server bound to `127.0.0.1` on an ephemeral port. It
//! accepts exactly one request — the provider's redirect after consent —
//! verifies the `state` parameter against the value we sent (CSRF defence),
//! extracts the authorization `code` (or an `error` such as `access_denied`),
//! writes a short human-facing page, and closes.
//!
//! No secret ever transits this channel: the `code` is a short-lived,
//! single-use grant that is only useful together with the PKCE verifier held
//! in memory. Even so, only `127.0.0.1` is bound, never a routable address.

use std::time::Duration;

use axomail_core::MailError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Upper bound on the request bytes read while looking for the request line.
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// The result of a successful (non-error) loopback exchange.
pub enum LoopbackResult {
    /// The provider returned an authorization code.
    Code(String),
    /// The user declined consent (`error=access_denied`).
    Denied,
}

/// A bound, not-yet-serving loopback receiver.
pub struct LoopbackServer {
    listener: TcpListener,
    redirect_uri: String,
}

impl LoopbackServer {
    /// Binds to `127.0.0.1` on an OS-assigned port.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Network`] if the socket cannot be bound.
    pub async fn bind() -> Result<Self, MailError> {
        let listener =
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(|_| MailError::Network {
                    message: "could not open a local port for the sign-in redirect".to_owned(),
                })?;
        let port = listener
            .local_addr()
            .map_err(|_| MailError::Network {
                message: "could not read the local redirect port".to_owned(),
            })?
            .port();
        Ok(Self {
            redirect_uri: format!("http://127.0.0.1:{port}"),
            listener,
        })
    }

    /// The `http://127.0.0.1:<port>` redirect URI to register in the
    /// authorization request.
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Serves exactly one redirect, bounded by `timeout`.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Timeout`] if no redirect arrives in time,
    /// [`MailError::Protocol`] on a malformed request or a `state` mismatch,
    /// and [`MailError::Network`] on a transport failure.
    pub async fn wait_for_redirect(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<LoopbackResult, MailError> {
        match tokio::time::timeout(timeout, self.accept_one(expected_state)).await {
            Ok(result) => result,
            Err(_) => Err(MailError::Timeout { after: timeout }),
        }
    }

    /// Accepts a single connection and resolves the redirect.
    async fn accept_one(&self, expected_state: &str) -> Result<LoopbackResult, MailError> {
        let (mut stream, _peer) = self
            .listener
            .accept()
            .await
            .map_err(|_| MailError::Network {
                message: "the sign-in redirect connection failed".to_owned(),
            })?;
        let target = read_request_target(&mut stream).await?;
        let outcome = resolve_target(&target, expected_state);
        // Always answer the browser so the user sees a clear result; the body
        // reflects success vs. failure but never contains secrets.
        let ok = matches!(outcome, Ok(LoopbackResult::Code(_)));
        let _ = write_response(&mut stream, ok).await;
        outcome
    }
}

/// Reads bytes until the first request line is complete and returns its
/// request target (the path plus query).
async fn read_request_target(stream: &mut TcpStream) -> Result<String, MailError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(line_end) = buf.iter().position(|&b| b == b'\n') {
            return extract_target(&buf[..line_end]);
        }
        if buf.len() > MAX_REQUEST_BYTES {
            return Err(MailError::Protocol {
                message: "the sign-in redirect request was malformed".to_owned(),
            });
        }
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| MailError::Network {
                message: "reading the sign-in redirect failed".to_owned(),
            })?;
        if read == 0 {
            return Err(MailError::Network {
                message: "the browser closed the sign-in redirect early".to_owned(),
            });
        }
        buf.extend_from_slice(&chunk[..read]);
    }
}

/// Extracts the request target from a `GET <target> HTTP/1.1` line.
fn extract_target(line: &[u8]) -> Result<String, MailError> {
    let text = String::from_utf8_lossy(line);
    let mut parts = text.split_whitespace();
    let malformed = || MailError::Protocol {
        message: "the sign-in redirect request was malformed".to_owned(),
    };
    let method = parts.next().ok_or_else(malformed)?;
    let target = parts.next().ok_or_else(malformed)?;
    if !method.eq_ignore_ascii_case("GET") {
        return Err(malformed());
    }
    Ok(target.to_owned())
}

/// Interprets the redirect target: verifies `state`, then returns the code,
/// a denial, or an error.
fn resolve_target(target: &str, expected_state: &str) -> Result<LoopbackResult, MailError> {
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in parse_query(query) {
        match key.as_str() {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }

    // The state check comes first: a mismatched or missing state means the
    // response is not the one we initiated and must be rejected outright.
    if state.as_deref() != Some(expected_state) {
        return Err(MailError::Protocol {
            message: "the sign-in response failed its anti-forgery check".to_owned(),
        });
    }
    if let Some(kind) = error {
        return if kind == "access_denied" {
            Ok(LoopbackResult::Denied)
        } else {
            Err(MailError::Authentication)
        };
    }
    match code {
        Some(code) if !code.is_empty() => Ok(LoopbackResult::Code(code)),
        _ => Err(MailError::Protocol {
            message: "the sign-in response contained no authorization code".to_owned(),
        }),
    }
}

/// Parses `a=b&c=d` query pairs, percent-decoding keys and values.
fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (percent_decode(key), percent_decode(value)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

/// Percent-decodes a query component, also mapping `+` to space.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a single hex digit.
fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Writes a minimal HTML response and closes the connection.
async fn write_response(stream: &mut TcpStream, ok: bool) -> Result<(), MailError> {
    let body = if ok {
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>axomail</title></head><body style=\"font-family:sans-serif\">\
         <h1>Signed in</h1><p>You can close this tab and return to axomail.</p>\
         </body></html>"
    } else {
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>axomail</title></head><body style=\"font-family:sans-serif\">\
         <h1>Sign-in not completed</h1><p>Return to axomail and try again.</p>\
         </body></html>"
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| MailError::Network {
            message: "could not write the sign-in confirmation page".to_owned(),
        })?;
    let _ = stream.flush().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn extracts_get_target() {
        assert_eq!(
            extract_target(b"GET /?code=abc&state=xyz HTTP/1.1\r").unwrap(),
            "/?code=abc&state=xyz"
        );
        assert!(extract_target(b"POST / HTTP/1.1").is_err());
    }

    #[test]
    fn resolves_code_when_state_matches() {
        let result = resolve_target("/?code=the-code&state=st", "st").unwrap();
        assert!(matches!(result, LoopbackResult::Code(c) if c == "the-code"));
    }

    #[test]
    fn rejects_state_mismatch() {
        assert!(matches!(
            resolve_target("/?code=x&state=evil", "st"),
            Err(MailError::Protocol { .. })
        ));
    }

    #[test]
    fn maps_access_denied_to_denied() {
        let result = resolve_target("/?error=access_denied&state=st", "st").unwrap();
        assert!(matches!(result, LoopbackResult::Denied));
    }

    #[test]
    fn percent_decodes_values() {
        assert_eq!(percent_decode("a%2Fb%20c"), "a/b c");
        assert_eq!(percent_decode("plus+space"), "plus space");
    }

    #[tokio::test]
    async fn serves_one_redirect_end_to_end() {
        let server = LoopbackServer::bind().await.unwrap();
        let addr = server
            .redirect_uri()
            .strip_prefix("http://")
            .unwrap()
            .to_owned();

        // A local client stands in for the browser's redirect GET.
        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET /?code=the-code&state=st HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            let mut response = Vec::new();
            let _ = stream.read_to_end(&mut response).await;
            response
        });

        let result = server
            .wait_for_redirect("st", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(matches!(result, LoopbackResult::Code(code) if code == "the-code"));

        let response = client.await.unwrap();
        assert!(String::from_utf8_lossy(&response).contains("Signed in"));
    }

    #[tokio::test]
    async fn times_out_when_no_redirect_arrives() {
        let server = LoopbackServer::bind().await.unwrap();
        let result = server
            .wait_for_redirect("st", Duration::from_millis(50))
            .await;
        assert!(matches!(result, Err(MailError::Timeout { .. })));
    }
}
