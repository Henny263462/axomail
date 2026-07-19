//! OAuth2 token endpoint exchange and refresh (RFC 6749) over reqwest.
//!
//! Both flows are `application/x-www-form-urlencoded` POSTs with PKCE (no
//! client secret). Responses are parsed into an [`OAuthTokens`] set with an
//! absolute expiry. Error responses are never echoed into a [`MailError`]:
//! token-endpoint error bodies can carry sensitive detail, so only a redacted
//! classification crosses the boundary.

use std::time::Duration;

use axomail_core::MailError;
use axomail_security::OAuthTokens;
use serde::Deserialize;

use super::{install_crypto_provider, unix_now};

/// Per-request network deadline for the token endpoint.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);

/// Fallback access-token lifetime if a provider omits `expires_in`.
const DEFAULT_EXPIRES_IN: i64 = 3600;

/// The subset of an RFC 6749 token response we consume.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Exchanges an authorization `code` for tokens (PKCE `authorization_code`).
///
/// # Errors
///
/// Returns [`MailError::Authentication`] if the endpoint rejects the grant,
/// [`MailError::Timeout`]/[`MailError::Network`] on transport failure, and
/// [`MailError::Protocol`] on an unparseable success body.
pub async fn exchange_code(
    token_url: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<OAuthTokens, MailError> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    post_token(token_url, &params, None).await
}

/// Refreshes tokens using a stored refresh token.
///
/// The provider's response may omit a rotated refresh token; in that case the
/// existing one is preserved.
///
/// # Errors
///
/// As for [`exchange_code`].
pub async fn refresh_tokens(
    token_url: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<OAuthTokens, MailError> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    post_token(token_url, &params, Some(refresh_token)).await
}

/// Posts a form to the token endpoint and maps the response to tokens.
async fn post_token(
    token_url: &str,
    params: &[(&str, &str)],
    fallback_refresh: Option<&str>,
) -> Result<OAuthTokens, MailError> {
    let client = http_client()?;
    let response = client
        .post(token_url)
        .form(params)
        .send()
        .await
        .map_err(map_reqwest)?;
    let status = response.status();
    if !status.is_success() {
        return Err(classify_token_error(status, response.headers()));
    }
    let body: TokenResponse = response.json().await.map_err(|_| MailError::Protocol {
        message: "the token endpoint returned an unexpected response".to_owned(),
    })?;
    let refresh_token = body
        .refresh_token
        .filter(|token| !token.is_empty())
        .or_else(|| fallback_refresh.map(str::to_owned))
        .unwrap_or_default();
    let expires_in = body.expires_in.unwrap_or(DEFAULT_EXPIRES_IN);
    Ok(OAuthTokens {
        access_token: body.access_token,
        refresh_token,
        expires_at_unix: unix_now().saturating_add(expires_in),
    })
}

/// Builds a reqwest client after ensuring the rustls provider is installed.
fn http_client() -> Result<reqwest::Client, MailError> {
    install_crypto_provider();
    reqwest::Client::builder()
        .timeout(TOKEN_TIMEOUT)
        .build()
        .map_err(|_| MailError::Network {
            message: "could not initialize the HTTP client".to_owned(),
        })
}

/// Classifies a non-2xx token-endpoint response into a redacted [`MailError`]
/// whose [`ErrorClass`](axomail_core::ErrorClass) drives retry behaviour.
///
/// - `429` → [`MailError::RateLimited`] (temporary; honours `Retry-After`),
/// - `5xx` → [`MailError::Network`] (temporary; the endpoint is unavailable),
/// - other `4xx` → [`MailError::Authentication`] (user-actionable): the grant
///   or client was rejected (e.g. `invalid_grant`/`invalid_client`), so retrying
///   without re-authorizing cannot help.
///
/// The response body is never read: it may carry sensitive detail, and the
/// status code alone is sufficient to classify.
fn classify_token_error(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> MailError {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return MailError::RateLimited {
            retry_after: parse_retry_after(headers),
        };
    }
    if status.is_server_error() {
        return MailError::Network {
            message: "the token endpoint is temporarily unavailable".to_owned(),
        };
    }
    MailError::Authentication
}

/// Parses a `Retry-After` header expressed as an integer number of seconds.
/// The HTTP-date form is not interpreted (it is rare for token endpoints); a
/// missing or unparseable header yields `None`.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds))
}

/// Maps a reqwest transport error to a redacted [`MailError`]. Transport
/// failures are temporary (timeout or network), so sync retries them with
/// backoff.
fn map_reqwest(error: reqwest::Error) -> MailError {
    if error.is_timeout() {
        MailError::Timeout {
            after: TOKEN_TIMEOUT,
        }
    } else {
        MailError::Network {
            message: "the token request could not be completed".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A one-shot plain-HTTP token endpoint returning `json_body`, plus the
    /// `http://127.0.0.1:<port>` URL to reach it.
    async fn scripted_token_endpoint(status_line: &'static str, json_body: &'static str) -> String {
        scripted_token_endpoint_with_headers(status_line, "", json_body).await
    }

    /// As [`scripted_token_endpoint`], but injects `extra_headers` (each a bare
    /// `Name: value` line without the trailing CRLF) into the response head.
    async fn scripted_token_endpoint_with_headers(
        status_line: &'static str,
        extra_headers: &'static str,
        json_body: &'static str,
    ) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Drain what is immediately available; enough to let reqwest finish
            // writing the request before we answer.
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let extra = if extra_headers.is_empty() {
                String::new()
            } else {
                format!("{extra_headers}\r\n")
            };
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\n{extra}\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{json_body}",
                json_body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        });
        url
    }

    #[tokio::test]
    async fn exchange_parses_tokens_and_sets_absolute_expiry() {
        let url = scripted_token_endpoint(
            "HTTP/1.1 200 OK",
            r#"{"access_token":"acc","refresh_token":"ref","expires_in":3600}"#,
        )
        .await;
        let before = unix_now();
        let tokens = exchange_code(&url, "client", "code", "http://127.0.0.1:1", "verifier")
            .await
            .unwrap();
        assert_eq!(tokens.access_token, "acc");
        assert_eq!(tokens.refresh_token, "ref");
        assert!(tokens.expires_at_unix >= before + 3600);
    }

    #[tokio::test]
    async fn refresh_preserves_existing_refresh_token_when_absent() {
        let url = scripted_token_endpoint(
            "HTTP/1.1 200 OK",
            r#"{"access_token":"acc2","expires_in":1800}"#,
        )
        .await;
        let tokens = refresh_tokens(&url, "client", "old-refresh").await.unwrap();
        assert_eq!(tokens.access_token, "acc2");
        // No refresh_token in the response: the old one is retained.
        assert_eq!(tokens.refresh_token, "old-refresh");
    }

    #[tokio::test]
    async fn invalid_grant_is_user_actionable_without_body() {
        let url = scripted_token_endpoint(
            "HTTP/1.1 400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"secret detail"}"#,
        )
        .await;
        let err = refresh_tokens(&url, "client", "bad").await.unwrap_err();
        assert!(matches!(err, MailError::Authentication));
        assert_eq!(err.class(), axomail_core::ErrorClass::UserConfiguration);
        assert!(!err.to_string().contains("secret detail"));
    }

    #[tokio::test]
    async fn unauthorized_client_is_user_actionable() {
        let url =
            scripted_token_endpoint("HTTP/1.1 401 Unauthorized", r#"{"error":"invalid_client"}"#)
                .await;
        let err = refresh_tokens(&url, "client", "bad").await.unwrap_err();
        assert!(matches!(err, MailError::Authentication));
        assert_eq!(err.class(), axomail_core::ErrorClass::UserConfiguration);
    }

    #[tokio::test]
    async fn rate_limited_is_temporary_and_carries_retry_after() {
        let url = scripted_token_endpoint_with_headers(
            "HTTP/1.1 429 Too Many Requests",
            "Retry-After: 30",
            r#"{"error":"rate_limited"}"#,
        )
        .await;
        let err = refresh_tokens(&url, "client", "r").await.unwrap_err();
        match err {
            MailError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(30)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn server_error_is_temporary_so_sync_retries() {
        let url = scripted_token_endpoint(
            "HTTP/1.1 503 Service Unavailable",
            r#"{"error":"temporarily_unavailable"}"#,
        )
        .await;
        let err = refresh_tokens(&url, "client", "r").await.unwrap_err();
        assert!(matches!(err, MailError::Network { .. }));
        assert!(err.is_temporary(), "5xx must be retryable");
    }

    #[tokio::test]
    async fn transport_failure_is_temporary() {
        // Bind then drop a listener to obtain a closed port, so the connection
        // is refused: a transport failure, which must classify as temporary.
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}");
        let err = refresh_tokens(&url, "client", "r").await.unwrap_err();
        assert!(
            matches!(err, MailError::Network { .. } | MailError::Timeout { .. }),
            "got {err:?}"
        );
        assert!(err.is_temporary());
    }
}
