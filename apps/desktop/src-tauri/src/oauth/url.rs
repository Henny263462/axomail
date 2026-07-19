//! Authorization-URL construction with dependency-free percent-encoding.

use axomail_core::{OAuthConfig, extra_auth_params_for};

/// Builds the provider consent URL for an authorization-code + PKCE request
/// (RFC 6749 §4.1 / RFC 7636).
///
/// All dynamic values are percent-encoded for the query component. `redirect_uri`
/// is the loopback address the local server is listening on, `state` is the
/// CSRF token, and `challenge` is the PKCE `S256` code challenge.
///
/// Provider-specific authorization parameters (e.g. Google's
/// `access_type=offline` and `prompt=consent`, which are what elicit a refresh
/// token) are appended from the matching [`OAuthPreset`](axomail_core::OAuthPreset)
/// so they apply identically on first sign-in and on re-authorization.
#[must_use]
pub fn build_authorize_url(
    config: &OAuthConfig,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    let separator = if config.auth_url.as_str().contains('?') {
        '&'
    } else {
        '?'
    };
    let scope = config.scopes.as_space_delimited();
    let params = [
        ("response_type", "code"),
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", redirect_uri),
        ("scope", &scope),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
    ];
    let mut pairs: Vec<String> = params
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect();
    for param in extra_auth_params_for(config.auth_url.as_str()) {
        pairs.push(format!(
            "{}={}",
            percent_encode(param.key()),
            percent_encode(param.value())
        ));
    }
    let query = pairs.join("&");
    format!("{}{separator}{query}", config.auth_url.as_str())
}

/// Percent-encodes `input` for use as a URL query value, leaving only the
/// RFC 3986 unreserved set (`ALPHA` / `DIGIT` / `-` `.` `_` `~`) untouched.
#[must_use]
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

/// Maps a nibble (0-15) to its uppercase hex digit.
fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use axomail_core::{HttpsUrl, OAuthClientId, OAuthProvider, OAuthScopes};

    fn config() -> OAuthConfig {
        OAuthConfig {
            client_id: OAuthClientId::parse("client-id-123").unwrap(),
            auth_url: HttpsUrl::parse("https://accounts.example.com/authorize").unwrap(),
            token_url: HttpsUrl::parse("https://accounts.example.com/token").unwrap(),
            scopes: OAuthScopes::new(vec![
                "https://mail.example.com/".to_owned(),
                "offline_access".to_owned(),
            ])
            .unwrap(),
        }
    }

    /// A config for `provider` using its preset endpoints and scopes.
    fn preset_config(provider: OAuthProvider) -> OAuthConfig {
        let preset = provider.preset().unwrap();
        OAuthConfig {
            client_id: OAuthClientId::parse("client-id-123").unwrap(),
            auth_url: preset.auth_url,
            token_url: preset.token_url,
            scopes: preset.scopes,
        }
    }

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(percent_encode("a b/c?d"), "a%20b%2Fc%3Fd");
        assert_eq!(percent_encode("plain-._~"), "plain-._~");
    }

    #[test]
    fn builds_expected_query() {
        let url = build_authorize_url(&config(), "http://127.0.0.1:5123", "st.ate", "chal");
        assert!(url.starts_with("https://accounts.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client-id-123"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5123"));
        assert!(url.contains("scope=https%3A%2F%2Fmail.example.com%2F%20offline_access"));
        assert!(url.contains("state=st.ate"));
        assert!(url.contains("code_challenge=chal"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn appends_with_ampersand_when_auth_url_has_query() {
        let mut cfg = config();
        cfg.auth_url = HttpsUrl::parse("https://accounts.example.com/authorize?tenant=x").unwrap();
        let url = build_authorize_url(&cfg, "http://127.0.0.1:1", "s", "c");
        assert!(url.contains("?tenant=x&response_type=code"));
    }

    #[test]
    fn gmail_url_requests_offline_access_and_consent() {
        let url = build_authorize_url(
            &preset_config(OAuthProvider::Gmail),
            "http://127.0.0.1:5123",
            "st",
            "ch",
        );
        assert!(url.contains("access_type=offline"), "url = {url}");
        assert!(url.contains("prompt=consent"), "url = {url}");
    }

    #[test]
    fn microsoft_url_carries_offline_access_scope_and_no_extra_params() {
        let url = build_authorize_url(
            &preset_config(OAuthProvider::Microsoft),
            "http://127.0.0.1:5123",
            "st",
            "ch",
        );
        // `offline_access` rides in the space-delimited scope, encoded as %20.
        assert!(url.contains("offline_access"), "url = {url}");
        assert!(!url.contains("access_type=offline"), "url = {url}");
        assert!(!url.contains("prompt=consent"), "url = {url}");
    }

    #[test]
    fn custom_endpoint_gets_no_provider_specific_params() {
        let url = build_authorize_url(&config(), "http://127.0.0.1:1", "s", "c");
        assert!(!url.contains("access_type=offline"));
        assert!(!url.contains("prompt=consent"));
    }
}
