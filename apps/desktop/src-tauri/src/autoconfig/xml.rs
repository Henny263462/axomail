//! Dependency-free parser for the Thunderbird autoconfig XML schema.
//!
//! The schema is small and well known, so a full XML library is unnecessary:
//! this scans for the handful of elements account setup needs
//! (`incomingServer`/`outgoingServer` blocks and their `hostname`, `port`,
//! `socketType`, `username`, and `authentication` children). It is forgiving —
//! unknown elements, attributes, and ordering are ignored — and total: any
//! input that does not yield a usable incoming *and* outgoing server returns
//! `None`, so the caller falls through to the next discovery source.
//!
//! Plaintext (`socketType` = `plain`) servers are never suggested: axomail does
//! not model insecure transports, so such a server is treated as unusable.

use axomail_core::{ConnectionSecurity, IncomingProtocol};

use super::{AutoconfigConfig, IncomingSuggestion, OutgoingSuggestion};

/// One `<tag …>…</tag>` element: its opening-tag attribute text and inner body.
struct Element<'a> {
    attrs: &'a str,
    inner: &'a str,
}

/// Parses a Thunderbird autoconfig document, resolving username placeholders
/// against `email`.
///
/// Returns `None` unless both a usable incoming and a usable outgoing server
/// are present.
#[must_use]
pub fn parse_autoconfig(xml: &str, email: &str) -> Option<AutoconfigConfig> {
    let incoming = pick_incoming(xml, email)?;
    let outgoing = pick_outgoing(xml, email)?;
    Some(AutoconfigConfig {
        display_name: child_text(xml, "displayName"),
        incoming,
        outgoing,
    })
}

/// Chooses the incoming server, preferring IMAP over POP3, skipping any block
/// that is not a usable (TLS/STARTTLS) server.
fn pick_incoming(xml: &str, email: &str) -> Option<IncomingSuggestion> {
    let servers: Vec<IncomingSuggestion> = elements(xml, "incomingServer")
        .into_iter()
        .filter_map(|element| parse_incoming(&element, email))
        .collect();
    servers
        .iter()
        .find(|server| server.protocol == IncomingProtocol::Imap)
        .or_else(|| servers.first())
        .cloned()
}

/// Chooses the first usable SMTP outgoing server.
fn pick_outgoing(xml: &str, email: &str) -> Option<OutgoingSuggestion> {
    elements(xml, "outgoingServer")
        .into_iter()
        .find_map(|element| parse_outgoing(&element, email))
}

/// Builds an [`IncomingSuggestion`] from one `incomingServer` element, or `None`
/// if it is not a usable IMAP/POP3 server.
fn parse_incoming(element: &Element<'_>, email: &str) -> Option<IncomingSuggestion> {
    let protocol = match attr(element.attrs, "type")?.to_ascii_lowercase().as_str() {
        "imap" => IncomingProtocol::Imap,
        "pop3" => IncomingProtocol::Pop3,
        _ => return None,
    };
    let host = non_empty(child_text(element.inner, "hostname")?)?;
    let port = parse_port(&child_text(element.inner, "port")?)?;
    let security = socket_type(&child_text(element.inner, "socketType")?)?;
    Some(IncomingSuggestion {
        protocol,
        host,
        port,
        security,
        username: resolve_username(child_text(element.inner, "username"), email),
        oauth: auth_is_oauth(element.inner),
    })
}

/// Builds an [`OutgoingSuggestion`] from one `outgoingServer` element, or `None`
/// if it is not a usable SMTP server.
fn parse_outgoing(element: &Element<'_>, email: &str) -> Option<OutgoingSuggestion> {
    if !attr(element.attrs, "type")?.eq_ignore_ascii_case("smtp") {
        return None;
    }
    let host = non_empty(child_text(element.inner, "hostname")?)?;
    let port = parse_port(&child_text(element.inner, "port")?)?;
    let security = socket_type(&child_text(element.inner, "socketType")?)?;
    Some(OutgoingSuggestion {
        host,
        port,
        security,
        username: resolve_username(child_text(element.inner, "username"), email),
        oauth: auth_is_oauth(element.inner),
    })
}

/// Returns every `<tag …>…</tag>` element in `xml` (the schema does not nest
/// these blocks, so a linear scan for matching open/close pairs suffices).
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<Element<'a>> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after_name = &rest[start + open.len()..];
        // Reject a longer element name (e.g. `<incomingServerX`): the char
        // right after the tag name must end the name.
        if !after_name
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
        {
            rest = after_name;
            continue;
        }
        let Some(gt) = after_name.find('>') else {
            break;
        };
        let attrs = after_name[..gt].trim_end_matches('/').trim();
        let body = &after_name[gt + 1..];
        match body.find(&close) {
            Some(end) => {
                out.push(Element {
                    attrs,
                    inner: &body[..end],
                });
                rest = &body[end + close.len()..];
            }
            None => break,
        }
    }
    out
}

/// Returns the decoded, trimmed text of the first `<tag>…</tag>` in `scope`.
fn child_text(scope: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = scope.find(&open)?;
    let after_name = &scope[start + open.len()..];
    if !after_name
        .chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
    {
        return None;
    }
    let gt = after_name.find('>')?;
    let body = &after_name[gt + 1..];
    let end = body.find(&close)?;
    let text = decode_entities(body[..end].trim());
    Some(text)
}

/// Returns every `<authentication>` value in an element body.
fn auth_is_oauth(inner: &str) -> bool {
    let mut rest = inner;
    while let Some(text) = child_text(rest, "authentication") {
        if text.eq_ignore_ascii_case("OAuth2") {
            return true;
        }
        // Advance past this element to inspect any siblings.
        match rest.find("</authentication>") {
            Some(idx) => rest = &rest[idx + "</authentication>".len()..],
            None => break,
        }
    }
    false
}

/// Reads a single- or double-quoted attribute value from opening-tag text.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let mut search = attrs;
    while let Some(idx) = search.find(&key) {
        // Ensure the match is a whole attribute name, not a suffix of another.
        let preceded_ok = idx == 0
            || search[..idx]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let after = &search[idx + key.len()..];
        let quote = after.chars().next();
        if preceded_ok && (quote == Some('"') || quote == Some('\'')) {
            let q = quote?;
            let value = &after[1..];
            let end = value.find(q)?;
            return Some(decode_entities(&value[..end]));
        }
        search = after;
    }
    None
}

/// Maps a Thunderbird `socketType` to a supported transport, or `None` for a
/// plaintext or unrecognized value (such a server is not suggestible).
fn socket_type(value: &str) -> Option<ConnectionSecurity> {
    match value.trim().to_ascii_uppercase().as_str() {
        "SSL" => Some(ConnectionSecurity::Tls),
        "STARTTLS" => Some(ConnectionSecurity::StartTls),
        _ => None,
    }
}

/// Parses a port, rejecting zero (an endpoint requires a non-zero port).
fn parse_port(value: &str) -> Option<u16> {
    match value.trim().parse::<u16>() {
        Ok(0) | Err(_) => None,
        Ok(port) => Some(port),
    }
}

/// Resolves the username template, substituting the address placeholders. An
/// absent or empty template defaults to the full email address.
fn resolve_username(template: Option<String>, email: &str) -> String {
    let template = template.unwrap_or_default();
    if template.trim().is_empty() {
        return email.to_owned();
    }
    let (local, domain) = email.rsplit_once('@').unwrap_or((email, ""));
    template
        .replace("%EMAILADDRESS%", email)
        .replace("%EMAILLOCALPART%", local)
        .replace("%EMAILDOMAIN%", domain)
}

/// Returns `Some` only for a non-empty string.
fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Decodes the five predefined XML entities. Numeric character references are
/// left as-is (they do not occur in real autoconfig hostnames or ports).
fn decode_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const IMAP_STARTTLS: &str = r#"<?xml version="1.0"?>
<clientConfig version="1.1">
  <emailProvider id="example.com">
    <domain>example.com</domain>
    <displayName>Example &amp; Co</displayName>
    <incomingServer type="imap">
      <hostname>mail.example.com</hostname>
      <port>143</port>
      <socketType>STARTTLS</socketType>
      <username>%EMAILLOCALPART%</username>
      <authentication>password-cleartext</authentication>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>mail.example.com</hostname>
      <port>587</port>
      <socketType>STARTTLS</socketType>
      <username>%EMAILADDRESS%</username>
      <authentication>password-cleartext</authentication>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

    #[test]
    fn parses_starttls_and_resolves_placeholders() {
        let config = parse_autoconfig(IMAP_STARTTLS, "klaus@example.com").expect("parses");
        assert_eq!(config.display_name.as_deref(), Some("Example & Co"));
        assert_eq!(config.incoming.host, "mail.example.com");
        assert_eq!(config.incoming.port, 143);
        assert_eq!(config.incoming.security, ConnectionSecurity::StartTls);
        assert_eq!(config.incoming.protocol, IncomingProtocol::Imap);
        assert_eq!(config.incoming.username, "klaus");
        assert!(!config.incoming.oauth);
        assert_eq!(config.outgoing.port, 587);
        assert_eq!(config.outgoing.username, "klaus@example.com");
    }

    #[test]
    fn prefers_imap_over_pop3() {
        let xml = r#"<clientConfig>
          <emailProvider>
            <incomingServer type="pop3"><hostname>pop.x.com</hostname><port>995</port><socketType>SSL</socketType></incomingServer>
            <incomingServer type="imap"><hostname>imap.x.com</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.com</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
          </emailProvider>
        </clientConfig>"#;
        let config = parse_autoconfig(xml, "a@x.com").expect("parses");
        assert_eq!(config.incoming.protocol, IncomingProtocol::Imap);
        assert_eq!(config.incoming.host, "imap.x.com");
    }

    #[test]
    fn falls_back_to_pop3_when_no_imap() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="pop3"><hostname>pop.x.com</hostname><port>995</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.com</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
          </emailProvider></clientConfig>"#;
        let config = parse_autoconfig(xml, "a@x.com").expect("parses");
        assert_eq!(config.incoming.protocol, IncomingProtocol::Pop3);
    }

    #[test]
    fn skips_plaintext_socket_type() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="imap"><hostname>imap.x.com</hostname><port>143</port><socketType>plain</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.com</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
          </emailProvider></clientConfig>"#;
        // The only incoming server is plaintext, so the document is unusable.
        assert!(parse_autoconfig(xml, "a@x.com").is_none());
    }

    #[test]
    fn detects_oauth2_authentication() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="imap"><hostname>imap.x.com</hostname><port>993</port><socketType>SSL</socketType>
              <authentication>OAuth2</authentication><authentication>password-cleartext</authentication></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.com</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
          </emailProvider></clientConfig>"#;
        let config = parse_autoconfig(xml, "a@x.com").expect("parses");
        assert!(config.incoming.oauth);
    }

    #[test]
    fn returns_none_for_garbage() {
        assert!(parse_autoconfig("<not really xml", "a@x.com").is_none());
        assert!(parse_autoconfig("", "a@x.com").is_none());
        assert!(parse_autoconfig("{\"json\":true}", "a@x.com").is_none());
    }

    #[test]
    fn does_not_match_longer_element_names() {
        // `<incomingServerConfig>` must not be treated as `<incomingServer>`.
        let xml = r#"<clientConfig><emailProvider>
            <incomingServerConfig>noise</incomingServerConfig>
            <incomingServer type="imap"><hostname>imap.x.com</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.com</hostname><port>465</port><socketType>SSL</socketType></outgoingServer>
          </emailProvider></clientConfig>"#;
        let config = parse_autoconfig(xml, "a@x.com").expect("parses");
        assert_eq!(config.incoming.host, "imap.x.com");
    }

    #[test]
    fn rejects_zero_and_out_of_range_ports() {
        assert!(parse_port("0").is_none());
        assert!(parse_port("70000").is_none());
        assert_eq!(parse_port(" 993 "), Some(993));
    }
}
