//! Curated common-provider fallback for account auto-configuration.
//!
//! When neither the domain's autoconfig nor the Mozilla ISP database yields a
//! configuration, this offers settings for a handful of large consumer
//! providers keyed by their well-known domains. The result is always labelled
//! as a guess in the UI so the user reviews and tests it before saving.
//!
//! Fabricating a generic `imap.<domain>` / `smtp.<domain>` guess for an
//! *unknown* domain is deliberately **not** done: an unverified, likely-wrong
//! host is worse than prompting the user to enter settings manually. Thunderbird
//! only presents such generic guesses after probing them live, which this
//! HTTPS-only lookup does not do — so an unknown domain returns `None`, and the
//! setup dialog falls back to manual entry.

use axomail_core::{ConnectionSecurity, IncomingProtocol};

use super::{AutoconfigConfig, IncomingSuggestion, OutgoingSuggestion};

/// A known provider's server settings. Hosts use the provider's standard IMAP
/// and SMTP endpoints; usernames are the full email address for every provider
/// listed here.
struct Preset {
    imap_host: &'static str,
    imap_port: u16,
    imap_security: ConnectionSecurity,
    smtp_host: &'static str,
    smtp_port: u16,
    smtp_security: ConnectionSecurity,
    /// Whether the provider's preferred authentication is OAuth2.
    oauth: bool,
}

/// Returns a curated configuration for a well-known provider domain, or `None`
/// when the domain is not one this fallback recognizes.
#[must_use]
pub fn guess_config(domain: &str, email: &str) -> Option<AutoconfigConfig> {
    let preset = preset_for(&domain.to_ascii_lowercase())?;
    Some(AutoconfigConfig {
        display_name: None,
        incoming: IncomingSuggestion {
            protocol: IncomingProtocol::Imap,
            host: preset.imap_host.to_owned(),
            port: preset.imap_port,
            security: preset.imap_security,
            username: email.to_owned(),
            oauth: preset.oauth,
        },
        outgoing: OutgoingSuggestion {
            host: preset.smtp_host.to_owned(),
            port: preset.smtp_port,
            security: preset.smtp_security,
            username: email.to_owned(),
            oauth: preset.oauth,
        },
    })
}

/// Implicit-TLS (`SSL`) IMAP+SMTP preset on the conventional 993/465 ports.
const fn tls(imap_host: &'static str, smtp_host: &'static str, oauth: bool) -> Preset {
    Preset {
        imap_host,
        imap_port: 993,
        imap_security: ConnectionSecurity::Tls,
        smtp_host,
        smtp_port: 465,
        smtp_security: ConnectionSecurity::Tls,
        oauth,
    }
}

/// Maps a recognized domain to its preset.
fn preset_for(domain: &str) -> Option<Preset> {
    let preset = match domain {
        "gmail.com" | "googlemail.com" => tls("imap.gmail.com", "smtp.gmail.com", true),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "hotmail.co.uk" => Preset {
            imap_host: "outlook.office365.com",
            imap_port: 993,
            imap_security: ConnectionSecurity::Tls,
            smtp_host: "smtp.office365.com",
            smtp_port: 587,
            smtp_security: ConnectionSecurity::StartTls,
            oauth: true,
        },
        "yahoo.com" | "yahoo.co.uk" | "yahoo.de" | "ymail.com" | "rocketmail.com" => {
            tls("imap.mail.yahoo.com", "smtp.mail.yahoo.com", false)
        }
        "aol.com" => tls("imap.aol.com", "smtp.aol.com", false),
        "icloud.com" | "me.com" | "mac.com" => Preset {
            imap_host: "imap.mail.me.com",
            imap_port: 993,
            imap_security: ConnectionSecurity::Tls,
            smtp_host: "smtp.mail.me.com",
            smtp_port: 587,
            smtp_security: ConnectionSecurity::StartTls,
            oauth: false,
        },
        "gmx.net" | "gmx.de" | "gmx.com" | "gmx.at" | "gmx.ch" => {
            tls("imap.gmx.net", "mail.gmx.net", false)
        }
        "web.de" => tls("imap.web.de", "smtp.web.de", false),
        "zoho.com" | "zohomail.com" => tls("imap.zoho.com", "smtp.zoho.com", false),
        "fastmail.com" | "fastmail.fm" => tls("imap.fastmail.com", "smtp.fastmail.com", false),
        "proton.me" | "protonmail.com" | "pm.me" => {
            // Proton requires the local Bridge, but 127.0.0.1 is a sane default
            // the user confirms; STARTTLS on the Bridge's advertised ports.
            Preset {
                imap_host: "127.0.0.1",
                imap_port: 1143,
                imap_security: ConnectionSecurity::StartTls,
                smtp_host: "127.0.0.1",
                smtp_port: 1025,
                smtp_security: ConnectionSecurity::StartTls,
                oauth: false,
            }
        }
        _ => return None,
    };
    Some(preset)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn known_provider_returns_full_config() {
        let config = guess_config("gmail.com", "klaus@gmail.com").expect("gmail is known");
        assert_eq!(config.incoming.host, "imap.gmail.com");
        assert_eq!(config.incoming.port, 993);
        assert_eq!(config.incoming.security, ConnectionSecurity::Tls);
        assert_eq!(config.incoming.protocol, IncomingProtocol::Imap);
        assert_eq!(config.incoming.username, "klaus@gmail.com");
        assert!(config.incoming.oauth);
        assert_eq!(config.outgoing.host, "smtp.gmail.com");
        assert_eq!(config.outgoing.username, "klaus@gmail.com");
    }

    #[test]
    fn domain_matching_is_case_insensitive() {
        assert!(guess_config("GoogleMail.com", "a@googlemail.com").is_some());
    }

    #[test]
    fn office365_uses_starttls_submission() {
        let config = guess_config("outlook.com", "a@outlook.com").expect("known");
        assert_eq!(config.incoming.host, "outlook.office365.com");
        assert_eq!(config.outgoing.port, 587);
        assert_eq!(config.outgoing.security, ConnectionSecurity::StartTls);
    }

    #[test]
    fn unknown_domain_returns_none() {
        assert!(guess_config("no-such-provider.example", "a@no-such-provider.example").is_none());
    }
}
