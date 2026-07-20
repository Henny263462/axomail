//! Mail account auto-configuration (Mozilla ISPDB / Thunderbird autoconfig).
//!
//! Given an email address, this module discovers a provider's incoming and
//! outgoing server settings so account setup can pre-fill the form. Discovery
//! is layered, tried in order and stopping at the first parseable result:
//!
//! 1. the domain's own autoconfig
//!    (`https://autoconfig.<domain>/mail/config-v1.1.xml`, then the
//!    `https://<domain>/.well-known/autoconfig/...` location);
//! 2. the Mozilla ISP database
//!    (`https://autoconfig.thunderbird.net/v1.1/<domain>`);
//! 3. a curated common-provider guess ([`guess`]).
//!
//! # Security
//!
//! - **HTTPS only + SSRF guard.** [`candidate_sources`] emits `https://` URLs
//!   exclusively, and every fetch (initial and each redirect hop) is validated
//!   under [`FetchPolicy::Guarded`]: the scheme must be `https`, the target host
//!   must not resolve to a loopback / private / link-local address, and a
//!   redirect is followed only when it stays on the same host over `https`. An
//!   attacker who controls the email domain (and thus `autoconfig.<domain>` /
//!   `<domain>`), or a hostile redirect, therefore cannot make the client reach
//!   `127.0.0.1` or an internal service, nor downgrade the fetch.
//! - **No credentials are ever sent.** Only the email *domain* appears in a
//!   lookup URL; the local part, password, and email address itself never
//!   leave the machine during discovery. The result is a *suggestion* the user
//!   reviews and tests before anything is stored.
//! - Response bodies are size-capped ([`MAX_BODY_BYTES`]) to bound a malicious
//!   or runaway endpoint, and only the domain (never the address) is logged.

mod guard;
mod guess;
mod xml;

use std::time::Duration;

use axomail_core::{ConnectionSecurity, IncomingProtocol, MailError};
use reqwest::Url;
use reqwest::header::LOCATION;

use guard::{may_follow_redirect, url_target_is_safe};

use crate::oauth::install_crypto_provider;

pub use guess::guess_config;
pub use xml::parse_autoconfig;

/// Per-request network deadline for one autoconfig fetch.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Largest response body accepted from an autoconfig endpoint. Real config
/// documents are a few kilobytes; this bounds a hostile or runaway server.
const MAX_BODY_BYTES: usize = 128 * 1024;

/// Upper bound on redirects followed during a single fetch.
const MAX_REDIRECTS: usize = 5;

/// How strictly a fetch validates its target host.
///
/// Production discovery uses [`Guarded`](FetchPolicy::Guarded): `https`-only, no
/// loopback/private/link-local targets, and same-host `https` redirects only —
/// the SSRF guard for a URL host that an attacker can steer via the email
/// domain. [`Unrestricted`](FetchPolicy::Unrestricted) exists only for tests that
/// drive a plaintext loopback stub server and must not be used in production.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPolicy {
    /// SSRF guard on: https-only, no private/loopback targets, no cross-host or
    /// scheme-downgrading redirects.
    Guarded,
    /// No guard — for tests driving a loopback HTTP stub.
    Unrestricted,
}

/// Which discovery source produced a configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoconfigSource {
    /// The mail domain's own autoconfig endpoint (authoritative).
    Autoconfig,
    /// The Mozilla ISP database.
    Ispdb,
    /// A curated common-provider guess (no server confirmed it).
    Guess,
}

impl AutoconfigSource {
    /// Stable machine identifier sent to the frontend, which localizes it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Autoconfig => "autoconfig",
            Self::Ispdb => "ispdb",
            Self::Guess => "guess",
        }
    }
}

/// A suggested incoming (IMAP/POP3) server configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSuggestion {
    /// Receiving protocol.
    pub protocol: IncomingProtocol,
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security (plaintext servers are dropped, never suggested).
    pub security: ConnectionSecurity,
    /// Login name, with any `%EMAILADDRESS%`-style placeholder resolved.
    pub username: String,
    /// Whether the provider's preferred authentication is OAuth2.
    pub oauth: bool,
}

/// A suggested outgoing (SMTP) server configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingSuggestion {
    /// Server host name.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Transport security (plaintext servers are dropped, never suggested).
    pub security: ConnectionSecurity,
    /// Login name, with any `%EMAILADDRESS%`-style placeholder resolved.
    pub username: String,
    /// Whether the provider's preferred authentication is OAuth2.
    pub oauth: bool,
}

/// A complete suggested configuration for one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoconfigConfig {
    /// Provider display name, when advertised.
    pub display_name: Option<String>,
    /// Incoming server suggestion.
    pub incoming: IncomingSuggestion,
    /// Outgoing server suggestion.
    pub outgoing: OutgoingSuggestion,
}

/// A successful discovery: the configuration and the source that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoconfigOutcome {
    /// Where the configuration came from.
    pub source: AutoconfigSource,
    /// The suggested configuration.
    pub config: AutoconfigConfig,
}

/// Extracts the lower-cased mail domain from an email address.
///
/// Returns `None` when the address has no `@`, an empty domain, or a domain
/// carrying whitespace — cases where no meaningful lookup can be built.
#[must_use]
pub fn parse_domain(email: &str) -> Option<String> {
    let (_local, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.trim();
    if domain.is_empty() || domain.chars().any(char::is_whitespace) {
        return None;
    }
    Some(domain.to_ascii_lowercase())
}

/// Builds the ordered `(source, url)` candidates for a domain.
///
/// Every URL is `https://` by construction; this is the single place lookup
/// URLs are minted, so the HTTPS-only guarantee lives here (proven by test).
#[must_use]
pub fn candidate_sources(domain: &str) -> Vec<(AutoconfigSource, String)> {
    vec![
        (
            AutoconfigSource::Autoconfig,
            format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
        ),
        (
            AutoconfigSource::Autoconfig,
            format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"),
        ),
        (
            AutoconfigSource::Ispdb,
            format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
        ),
    ]
}

/// Discovers a configuration for `email`, or `None` when nothing is found.
///
/// Tries the network sources first, then the common-provider guess. Network
/// and parse failures are swallowed (the next source is tried); this function
/// never returns an error, only a best-available suggestion or `None`.
pub async fn discover(email: &str) -> Option<AutoconfigOutcome> {
    let domain = parse_domain(email)?;
    tracing::debug!(%domain, "autoconfig lookup started");

    let network = match http_client() {
        Ok(client) => {
            discover_from_sources(
                &client,
                &candidate_sources(&domain),
                email,
                FetchPolicy::Guarded,
            )
            .await
        }
        Err(_) => None,
    };
    if let Some(found) = network {
        tracing::debug!(%domain, source = found.source.as_str(), "autoconfig resolved");
        return Some(found);
    }

    if let Some(config) = guess_config(&domain, email) {
        tracing::debug!(%domain, "autoconfig resolved via common-provider guess");
        return Some(AutoconfigOutcome {
            source: AutoconfigSource::Guess,
            config,
        });
    }

    tracing::debug!(%domain, "autoconfig not found");
    None
}

/// Fetches each source in order under `policy`, returning the first that yields
/// a parseable configuration. The HTTP client is injected so tests can drive
/// this against a local stub without contacting the network; production passes
/// [`FetchPolicy::Guarded`] so every hop is SSRF-checked.
pub async fn discover_from_sources(
    client: &reqwest::Client,
    sources: &[(AutoconfigSource, String)],
    email: &str,
    policy: FetchPolicy,
) -> Option<AutoconfigOutcome> {
    for (source, url) in sources {
        let Some(body) = fetch_body(client, url, policy).await else {
            continue;
        };
        if let Some(config) = parse_autoconfig(&body, email) {
            return Some(AutoconfigOutcome {
                source: *source,
                config,
            });
        }
    }
    None
}

/// Builds the production HTTP client: automatic redirects **disabled** (each hop
/// is validated by [`fetch_body`] instead), a per-request timeout, and the ring
/// crypto provider installed for rustls.
fn http_client() -> Result<reqwest::Client, MailError> {
    install_crypto_provider();
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        // Redirects are handled manually so every hop can be re-validated for
        // scheme, host, and target-IP safety (SSRF guard). reqwest's own policy
        // cannot resolve+reject private targets or forbid cross-host hops.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| MailError::Network {
            message: "could not initialize the HTTP client".to_owned(),
        })
}

/// Performs a guarded GET, following at most [`MAX_REDIRECTS`] redirects, and
/// returns the body on a final 2xx or `None` on any other status, an unsafe
/// target, or a transport failure. The body is read with a hard size cap.
///
/// Every hop is validated before the request is issued: the URL must be
/// `https`, and its host must not resolve to a loopback, private, or link-local
/// address (SSRF guard — an attacker who controls the email domain, or a hostile
/// redirect, must not be able to reach `127.0.0.1` or an internal service). A
/// redirect is followed only when it stays on the **same host** over `https`.
/// Under [`FetchPolicy::Unrestricted`] (tests only) the guard is skipped and a
/// single plain GET is performed against a loopback stub.
async fn fetch_body(client: &reqwest::Client, url: &str, policy: FetchPolicy) -> Option<String> {
    if policy == FetchPolicy::Unrestricted {
        let response = client.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        return read_capped(response).await;
    }

    let mut current = Url::parse(url).ok()?;
    for _ in 0..=MAX_REDIRECTS {
        if !url_target_is_safe(&current).await {
            return None;
        }
        let response = client.get(current.clone()).send().await.ok()?;
        if response.status().is_redirection() {
            let location = response.headers().get(LOCATION)?.to_str().ok()?;
            let next = current.join(location).ok()?;
            if !may_follow_redirect(&current, &next) {
                return None;
            }
            current = next;
            continue;
        }
        if !response.status().is_success() {
            return None;
        }
        return read_capped(response).await;
    }
    None
}

/// Reads a response body up to [`MAX_BODY_BYTES`], returning `None` if the body
/// exceeds the cap, is not valid UTF-8, or the stream errors mid-read.
async fn read_capped(mut response: reqwest::Response) -> Option<String> {
    let mut buffer = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if buffer.len() + chunk.len() > MAX_BODY_BYTES {
                    return None;
                }
                buffer.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => return None,
        }
    }
    String::from_utf8(buffer).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_domain_extracts_and_lowercases() {
        assert_eq!(
            parse_domain("Klaus@Example.COM").as_deref(),
            Some("example.com")
        );
        assert_eq!(parse_domain("  a@b.de  ").as_deref(), Some("b.de"));
    }

    #[test]
    fn parse_domain_rejects_malformed() {
        assert!(parse_domain("no-at-sign").is_none());
        assert!(parse_domain("trailing@").is_none());
        assert!(parse_domain("space@ex ample.com").is_none());
    }

    #[test]
    fn candidate_sources_are_https_only() {
        let sources = candidate_sources("example.com");
        assert_eq!(sources.len(), 3);
        assert!(
            sources.iter().all(|(_, url)| url.starts_with("https://")),
            "every autoconfig lookup URL must be HTTPS: {sources:?}"
        );
        // The ISPDB path carries only the domain — never the email address.
        assert!(sources.iter().any(|(source, url)| {
            *source == AutoconfigSource::Ispdb
                && url == "https://autoconfig.thunderbird.net/v1.1/example.com"
        }));
    }

    #[test]
    fn source_labels_are_stable() {
        assert_eq!(AutoconfigSource::Autoconfig.as_str(), "autoconfig");
        assert_eq!(AutoconfigSource::Ispdb.as_str(), "ispdb");
        assert_eq!(AutoconfigSource::Guess.as_str(), "guess");
    }
}
