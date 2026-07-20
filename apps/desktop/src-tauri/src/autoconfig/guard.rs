//! SSRF guard for autoconfig fetches.
//!
//! An autoconfig lookup targets a host an attacker can steer via the email
//! domain (`autoconfig.<domain>` / `<domain>`), and a hostile endpoint can issue
//! redirects. These predicates keep such a fetch from reaching internal
//! services: [`url_target_is_safe`] rejects any target whose host resolves to a
//! loopback, private, or link-local address (and requires `https`), and
//! [`may_follow_redirect`] permits only same-host `https` redirects.

use std::net::{IpAddr, Ipv6Addr};

use reqwest::Url;

/// Whether a redirect from `from` to `to` may be followed: it must stay on the
/// exact same host and use `https`. A cross-host or scheme-downgrading redirect
/// is refused so a hostile endpoint cannot bounce the fetch to an internal host.
pub(super) fn may_follow_redirect(from: &Url, to: &Url) -> bool {
    to.scheme() == "https" && to.host_str().is_some() && to.host_str() == from.host_str()
}

/// Whether it is safe to connect to `url`'s target: `https`, a resolvable host,
/// and **no** resolved address in a loopback/private/link-local range. Resolving
/// here (rather than trusting reqwest) lets a hostname that points at an internal
/// IP be rejected before any connection is made.
pub(super) async fn url_target_is_safe(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let port = url.port_or_known_default().unwrap_or(443);

    // An IP literal is checked directly; a hostname is resolved and every
    // resolved address must be permitted (reject-if-any-forbidden defeats a
    // resolver that returns a public address alongside an internal one).
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_forbidden_ip(ip);
    }
    match tokio::net::lookup_host((host, port)).await {
        Ok(addrs) => {
            let mut any = false;
            for addr in addrs {
                any = true;
                if is_forbidden_ip(addr.ip()) {
                    return false;
                }
            }
            any
        }
        Err(_) => false,
    }
}

/// Whether an IP address is in a range the autoconfig fetcher must never reach:
/// loopback, private/unique-local, link-local, or unspecified.
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // Carrier-grade NAT / shared address space (100.64.0.0/10).
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40)
        }
        IpAddr::V6(v6) => is_forbidden_ipv6(v6),
    }
}

/// IPv6 forbidden-range check: loopback (`::1`), unspecified (`::`), unique-local
/// (`fc00::/7`), link-local (`fe80::/10`), and IPv4-mapped addresses whose inner
/// IPv4 is itself forbidden.
fn is_forbidden_ipv6(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() {
        return true;
    }
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_forbidden_ip(IpAddr::V4(v4));
    }
    let first = v6.segments()[0];
    // fc00::/7 (unique local) and fe80::/10 (link-local unicast).
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn forbidden_ip_ranges_are_rejected() {
        // SECURITY (finding #11): loopback, private, link-local, unspecified.
        for forbidden in [
            "127.0.0.1",
            "127.5.6.7",
            "10.0.0.1",
            "172.16.4.9",
            "192.168.1.1",
            "169.254.1.1",
            "0.0.0.0",
            "100.64.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:127.0.0.1",
        ] {
            let ip: IpAddr = forbidden.parse().unwrap();
            assert!(is_forbidden_ip(ip), "{forbidden} must be forbidden");
        }
        // Public addresses are permitted.
        for allowed in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:4700::1111"] {
            let ip: IpAddr = allowed.parse().unwrap();
            assert!(!is_forbidden_ip(ip), "{allowed} should be allowed");
        }
    }

    #[tokio::test]
    async fn loopback_and_private_ip_literal_targets_are_refused() {
        // SECURITY (finding #11): a@127.0.0.1 or a private-IP domain must be
        // refused before any connection is attempted.
        assert!(!url_target_is_safe(&Url::parse("https://127.0.0.1/x").unwrap()).await);
        assert!(!url_target_is_safe(&Url::parse("https://10.0.0.1/x").unwrap()).await);
        assert!(!url_target_is_safe(&Url::parse("https://[::1]/x").unwrap()).await);
        // A non-https target is refused even if the host is public.
        assert!(!url_target_is_safe(&Url::parse("http://8.8.8.8/x").unwrap()).await);
        // A public IP literal over https is allowed.
        assert!(url_target_is_safe(&Url::parse("https://8.8.8.8/x").unwrap()).await);
    }

    #[test]
    fn redirects_only_follow_same_host_https() {
        // SECURITY (finding #11): a cross-host or scheme-downgrading redirect is
        // not followed; a same-host https redirect is.
        let from = Url::parse("https://autoconfig.example.com/a").unwrap();
        assert!(may_follow_redirect(
            &from,
            &Url::parse("https://autoconfig.example.com/b").unwrap()
        ));
        assert!(!may_follow_redirect(
            &from,
            &Url::parse("https://evil.internal/b").unwrap()
        ));
        assert!(!may_follow_redirect(
            &from,
            &Url::parse("http://autoconfig.example.com/b").unwrap()
        ));
        assert!(!may_follow_redirect(
            &from,
            &Url::parse("https://127.0.0.1/b").unwrap()
        ));
    }
}
