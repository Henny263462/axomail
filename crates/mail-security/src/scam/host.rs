//! Host and URL parsing helpers for the deceptive-link pass.
//!
//! Everything here is pure string work with no dependencies: extract a host
//! from a URL or from bare visible text, recognize IP literals, and reduce a
//! host to its registrable domain via a public-suffix-free heuristic. See the
//! parent module for how these feed the two phishing heuristics.

/// Whether `value` begins with an `http`/`https` scheme (case-insensitive).
pub(super) fn is_http_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Extracts the host of an absolute `http(s)` URL: authority minus userinfo
/// and port, lowercased, trailing dot removed. Returns `None` if empty.
pub(super) fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority_host(authority)
}

/// Reduces an authority (`user:pass@host:port`) to just its lowercased host,
/// handling bracketed IPv6 literals.
fn authority_host(authority: &str) -> Option<String> {
    let after_userinfo = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if after_userinfo.starts_with('[') {
        // Bracketed IPv6 literal: keep the brackets, drop any trailing port.
        match after_userinfo.find(']') {
            Some(close) => &after_userinfo[..=close],
            None => after_userinfo,
        }
    } else {
        after_userinfo.split(':').next().unwrap_or("")
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Interprets fully-URL-or-host visible text as a host, or `None`.
///
/// The whole trimmed text must be a single token (no internal whitespace) that
/// is either an explicit `http(s)` URL, or a bare host whose final label is a
/// recognized TLD (or an IP literal). This keeps prose and file names from
/// being read as links.
pub(super) fn parse_text_host(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    if is_http_url(trimmed) {
        return url_host(trimmed).filter(|h| is_ip_literal(h) || looks_like_hostname(h));
    }
    let authority = trimmed.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority_host(authority)?;
    (is_ip_literal(&host) || looks_like_hostname(&host)).then_some(host)
}

/// Whether `host` looks like a domain name: dotted labels of alphanumerics and
/// hyphens whose final label is a known TLD. IP literals are handled elsewhere.
fn looks_like_hostname(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let all_labels_valid = labels.iter().all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    let tld = labels[labels.len() - 1];
    all_labels_valid && is_known_tld(tld)
}

/// The registrable domain of `host` (eTLD+1), by a public-suffix-free
/// heuristic: last two labels, or three when the last two form a curated
/// two-level suffix. IP literals compare whole.
pub(super) fn registrable_domain(host: &str) -> String {
    if is_ip_literal(host) {
        return host.to_owned();
    }
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    let n = labels.len();
    if n <= 2 {
        return labels.join(".");
    }
    let last_two = labels[n - 2..].join(".");
    if TWO_LEVEL_SUFFIXES.contains(&last_two.as_str()) {
        labels[n - 3..].join(".")
    } else {
        last_two
    }
}

/// Whether `host` is a raw IPv4 (`a.b.c.d`, each 0-255) or a bracketed IPv6
/// literal (`[...:...]`).
pub(super) fn is_ip_literal(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.contains(':') && inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':');
    }
    let octets: Vec<&str> = host.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|octet| {
            !octet.is_empty()
                && octet.chars().all(|c| c.is_ascii_digit())
                && octet.parse::<u8>().is_ok()
        })
}

/// Decodes the small set of HTML entities the sanitizer can emit, plus numeric
/// character references. Unknown entities are left verbatim.
pub(super) fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        if let Some(semi) = tail.find(';').filter(|&s| s <= 10) {
            let entity = &tail[1..semi];
            if let Some(decoded) = decode_one_entity(entity) {
                out.push(decoded);
                rest = &tail[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// Decodes a single entity body (the text between `&` and `;`).
fn decode_one_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = entity.strip_prefix('#')?;
            let code = if let Some(hex) = number.strip_prefix(['x', 'X']) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                number.parse::<u32>().ok()?
            };
            char::from_u32(code)
        }
    }
}

/// Curated common TLDs. Used only to decide whether *scheme-less* visible text
/// is a host; explicit `http(s)` text bypasses this list. Kept intentionally
/// small — a miss only means one fewer warning, never a wrong strip.
const KNOWN_TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "mil", "int", "io", "co", "info", "biz", "app", "dev",
    "xyz", "online", "site", "shop", "store", "cloud", "email", "bank", "de", "uk", "eu", "fr",
    "nl", "us", "ca", "au", "jp", "cn", "ru", "br", "in", "it", "es", "se", "ch", "at", "be", "pl",
    "kr", "mx", "za", "nz", "ie", "no", "fi", "dk", "pt", "cz", "gr", "tr", "il", "sg",
];

/// Two-level public suffixes where the registrable domain is the last three
/// labels. Not exhaustive; covers the common cases without a PSL dependency.
const TWO_LEVEL_SUFFIXES: &[&str] = &[
    "co.uk", "org.uk", "gov.uk", "ac.uk", "me.uk", "ltd.uk", "plc.uk", "com.au", "net.au",
    "org.au", "gov.au", "edu.au", "co.jp", "or.jp", "ne.jp", "go.jp", "com.br", "com.cn", "co.in",
    "co.nz", "co.za", "com.mx", "com.tr", "co.il", "com.sg", "co.kr", "com.hk",
];

/// Whether `tld` (already lowercased) is a recognized top-level domain.
fn is_known_tld(tld: &str) -> bool {
    KNOWN_TLDS.contains(&tld)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_literals_are_recognized() {
        assert!(is_ip_literal("192.168.0.1"));
        assert!(is_ip_literal("8.8.8.8"));
        assert!(is_ip_literal("[2001:db8::1]"));
        assert!(!is_ip_literal("256.1.1.1")); // 256 is out of range
        assert!(!is_ip_literal("example.com"));
        assert!(!is_ip_literal("1.2.3")); // too few octets
    }

    #[test]
    fn registrable_domain_uses_two_and_three_labels() {
        assert_eq!(registrable_domain("paypal.com"), "paypal.com");
        assert_eq!(registrable_domain("www.paypal.com"), "paypal.com");
        assert_eq!(registrable_domain("login.secure.paypal.com"), "paypal.com");
        assert_eq!(registrable_domain("example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("a.b.example.co.uk"), "example.co.uk");
    }

    #[test]
    fn parse_text_host_rejects_prose_and_file_names() {
        assert_eq!(parse_text_host("Click here"), None);
        assert_eq!(parse_text_host("report.pdf"), None); // pdf is not a TLD
        assert_eq!(parse_text_host("2.0"), None);
        assert_eq!(parse_text_host("paypal.com").as_deref(), Some("paypal.com"));
        assert_eq!(
            parse_text_host("https://paypal.com/login").as_deref(),
            Some("paypal.com")
        );
    }

    #[test]
    fn url_host_strips_userinfo_and_port() {
        assert_eq!(
            url_host("https://user:pw@Example.COM:8443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("http://[2001:db8::1]:80/").as_deref(),
            Some("[2001:db8::1]")
        );
    }
}
