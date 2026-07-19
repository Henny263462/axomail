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

/// Scans visible text for **embedded** host or URL tokens, returning each
/// distinct host found (in order of first appearance).
///
/// Unlike [`parse_text_host`], which requires the *entire* trimmed text to be a
/// single host, this splits on whitespace and tests each token, so a host
/// buried in prose (`Visit paypal.com now`) is still recovered — closing the
/// whitespace bypass. Each token is stripped of wrapping punctuation and then
/// held to the same conservative gate ([`parse_text_host`]): an `http(s)` URL,
/// or a bare host whose final label is a recognized TLD (or an IP literal). So
/// prose like `read my essay.Really` still yields nothing (`really` is not a
/// TLD).
pub(super) fn scan_text_hosts(text: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if let Some(host) = parse_text_host(token)
            && !hosts.contains(&host)
        {
            hosts.push(host);
        }
    }
    hosts
}

/// Whether `registrable` is a curated email-service-provider click-tracker or
/// link-redirector domain, on which a link-text/href mismatch is expected and
/// should not raise a deceptive-link warning. See [`REDIRECTOR_DOMAINS`].
pub(super) fn is_known_redirector(registrable: &str) -> bool {
    REDIRECTOR_DOMAINS.contains(&registrable)
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

/// Whether `host` is an IP address literal, including the non-standard numeric
/// forms that browsers and resolvers still accept and phishers therefore use:
///
/// - dotted-quad IPv4 (`192.168.0.1`);
/// - a single packed integer (`2130706433`) in decimal, `0x` hex
///   (`0x7f000001`), or leading-zero octal;
/// - dotted forms with hex/octal octets (`0x7f.0x0.0x0.0x1`, `0177.0.0.1`);
/// - a bracketed IPv6 literal, including the IPv4-mapped tail form
///   (`[::ffff:169.254.169.254]`).
///
/// It leans on [`std::net`] parsing for the canonical forms and a small
/// base-aware parser for the rest. This is warning-only, so a rare miss or an
/// over-broad match on a purely numeric non-IP host is harmless.
pub(super) fn is_ip_literal(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        // std parses the mixed `x:x:...:d.d.d.d` form, so the embedded-IPv4
        // tail is covered here rather than by a bespoke scan.
        return inner.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    is_nonstandard_numeric_ipv4(host)
}

/// Whether `host` is a non-standard-but-resolvable numeric IPv4 encoding: one
/// to four dot-separated labels, each a number in decimal, `0x` hex, or
/// leading-zero octal, packing into 32 bits. Plain multi-label decimals are
/// rejected here (they are handled by [`std::net::Ipv4Addr`] above, which also
/// rejects malformed ones like `2.0` or `256.1.1.1`), so this only adds the
/// alternative-base and single-integer encodings.
fn is_nonstandard_numeric_ipv4(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.is_empty() || labels.len() > 4 {
        return false;
    }
    let mut saw_alt_base = false;
    let mut values = Vec::with_capacity(labels.len());
    for label in &labels {
        let Some((value, alt)) = parse_numeric_label(label) else {
            return false;
        };
        saw_alt_base |= alt;
        values.push(value);
    }
    match values.as_slice() {
        // A lone packed integer that fits in 32 bits (e.g. 2130706433 or its
        // hex/octal spelling).
        [dword] => *dword <= u64::from(u32::MAX),
        // Multi-label forms only count when at least one octet used an
        // alternative base; pure decimals reaching here failed the std parse
        // and are not IPs.
        _ if !saw_alt_base => false,
        [a, b] => *a <= 0xFF && *b <= 0x00FF_FFFF,
        [a, b, c] => *a <= 0xFF && *b <= 0xFF && *c <= 0xFFFF,
        [a, b, c, d] => [a, b, c, d].iter().all(|v| **v <= 0xFF),
        _ => false,
    }
}

/// Parses one numeric host label, returning its value and whether an
/// alternative base marker (`0x` or a leading-zero octal) was used. Decimal is
/// the default; an empty or non-numeric label yields `None`.
fn parse_numeric_label(label: &str) -> Option<(u64, bool)> {
    if label.is_empty() {
        return None;
    }
    let lower = label.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        return (!hex.is_empty())
            .then(|| u64::from_str_radix(hex, 16).ok())
            .flatten()
            .map(|value| (value, true));
    }
    if lower.len() > 1 && lower.starts_with('0') {
        // A leading zero marks octal; all digits must be valid octal.
        return u64::from_str_radix(&lower, 8)
            .ok()
            .map(|value| (value, true));
    }
    lower.parse::<u64>().ok().map(|value| (value, false))
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

/// Registrable domains of well-known email-service-provider (ESP) click
/// trackers and link redirectors. These legitimately rewrite every link so the
/// visible text names the destination brand while the `href` points at the
/// tracker's own domain — exactly the shape the deceptive-link heuristic keys
/// on — so a text/href registrable-domain mismatch on one of these is expected
/// noise, not phishing, and the warning is suppressed for it.
///
/// This is a deliberate noise/value tradeoff: the same curated-list approach as
/// [`TWO_LEVEL_SUFFIXES`], with no public-suffix or allowlist service. It is not
/// exhaustive; a missing redirector only means one extra (false) warning, never
/// a suppressed real one for a non-listed domain.
const REDIRECTOR_DOMAINS: &[&str] = &[
    "list-manage.com",
    "sendgrid.net",
    "mandrillapp.com",
    "mailchimp.com",
    "hubspotlinks.com",
    "klaviyomail.com",
    "cmail19.com",
    "cmail20.com",
    "mailgun.info",
    "rs6.net",
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
    fn recognizes_nonstandard_numeric_ip_forms() {
        // A single packed decimal integer (127.0.0.1).
        assert!(is_ip_literal("2130706433"));
        // The same value in hex and octal.
        assert!(is_ip_literal("0x7f000001"));
        assert!(is_ip_literal("017700000001"));
        // Dotted forms with hex/octal octets.
        assert!(is_ip_literal("0x7f.0x0.0x0.0x1"));
        assert!(is_ip_literal("0177.0.0.1"));
        // Bracketed IPv6 with an embedded IPv4 tail (link-local metadata IP).
        assert!(is_ip_literal("[::ffff:169.254.169.254]"));
    }

    #[test]
    fn numeric_ip_detection_does_not_swallow_ordinary_hosts() {
        // Version-like and dotted-decimal non-IPs stay non-IP so they are not
        // mistaken for hosts by the text scanner.
        assert!(!is_ip_literal("2.0"));
        assert!(!is_ip_literal("1.2.3.4.5"));
        assert!(!is_ip_literal("example.com"));
        assert!(!is_ip_literal("999.1")); // pure decimals with no alt base
    }

    #[test]
    fn scans_embedded_host_tokens_out_of_prose() {
        // A host buried in prose (with surrounding punctuation) is recovered.
        assert_eq!(
            scan_text_hosts("Please visit paypal.com now to verify"),
            vec!["paypal.com".to_owned()]
        );
        assert_eq!(
            scan_text_hosts("Go to (https://paypal.com/login) immediately"),
            vec!["paypal.com".to_owned()]
        );
        // Conservative gate: prose whose dotted token is not a TLD stays empty.
        assert!(scan_text_hosts("read my essay.Really it is good").is_empty());
        assert!(scan_text_hosts("see report.pdf attached").is_empty());
    }

    #[test]
    fn known_redirectors_are_recognized_by_registrable_domain() {
        assert!(is_known_redirector(&registrable_domain(
            "click.list-manage.com"
        )));
        assert!(is_known_redirector(&registrable_domain(
            "url1234.sendgrid.net"
        )));
        assert!(!is_known_redirector(&registrable_domain("evil.example")));
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
