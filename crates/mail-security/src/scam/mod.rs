//! Deceptive-link ("scam") detection over sanitized message HTML.
//!
//! This is a *warning-only* sibling pass to [`crate::html`]: it never blocks,
//! rewrites, or removes anything. It walks the already-sanitized DOM (a clean,
//! balanced, entity-escaped fragment, so scanning it is reliable) and reports
//! two well-understood phishing tells:
//!
//! 1. **Deceptive link text** — an anchor whose *visible text* itself reads as
//!    a URL or bare host whose registrable domain differs from the `href`'s
//!    (e.g. text `paypal.com` linking to `evil.example`).
//! 2. **Numeric-IP host** — an anchor whose `href` host is a raw IPv4/IPv6
//!    literal rather than a name, a common credential-harvesting pattern.
//!
//! Host and registrable-domain parsing lives in the [`host`] submodule; this
//! module owns the anchor scan and the two heuristics. The registrable-domain
//! comparison is a public-suffix-free heuristic (see [`host`]), and bare
//! visible text is only treated as a host when its final label is a recognized
//! TLD, so ordinary prose and file names are never mistaken for a link. The
//! result is a list the reading pane surfaces as a warning banner.

mod host;

use host::{
    decode_entities, is_http_url, is_ip_literal, parse_text_host, registrable_domain, url_host,
};

/// The kind of a detected scam indicator. Stable identifiers so the frontend
/// can style or localize each without parsing the human-readable detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScamIndicatorKind {
    /// Visible link text names a different site than the link points to.
    DeceptiveLink,
    /// The link's host is a raw IP address literal, not a domain name.
    IpAddressHost,
}

impl ScamIndicatorKind {
    /// A stable machine-readable slug (camelCase to match the DTO contract).
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::DeceptiveLink => "deceptiveLink",
            Self::IpAddressHost => "ipAddressHost",
        }
    }
}

/// One reason a message was flagged as potentially deceptive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScamIndicator {
    /// The category of the indicator.
    pub kind: ScamIndicatorKind,
    /// A human-readable, secret-free explanation for the warning banner.
    pub detail: String,
}

/// Scans sanitized HTML and returns every distinct scam indicator found.
///
/// The input must be sanitizer output (see the module docs): tags balanced,
/// text entity-escaped, links carrying only `http`/`https`/`mailto` targets.
/// The scan is pure, total, and never panics.
#[must_use]
pub fn detect_scam_indicators(sanitized_html: &str) -> Vec<ScamIndicator> {
    let lower = sanitized_html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out: Vec<ScamIndicator> = Vec::new();
    let mut search = 0;

    while let Some(rel) = lower[search..].find("<a") {
        let tag_start = search + rel;
        let after_name = tag_start + 2;
        // Require a tag-name boundary so `<article>`/`<abbr>` never match.
        if !matches!(bytes.get(after_name), Some(b) if is_tag_boundary(*b)) {
            search = after_name;
            continue;
        }
        let Some(gt_rel) = lower[after_name..].find('>') else {
            break;
        };
        let open_end = after_name + gt_rel; // index of '>'
        let open_tag = &sanitized_html[tag_start..=open_end];

        let content_start = open_end + 1;
        let Some(close_rel) = lower[content_start..].find("</a") else {
            break;
        };
        let content_end = content_start + close_rel;
        let content = &sanitized_html[content_start..content_end];
        search = (content_end + 3).min(sanitized_html.len());

        if let Some(href) = extract_attr(open_tag, "href") {
            evaluate_anchor(&href, content, &mut out);
        }
    }
    out
}

/// Bytes that may terminate a tag name (`<a` must be followed by one).
fn is_tag_boundary(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'>' | b'/')
}

/// Applies both heuristics to one anchor, pushing any distinct indicators.
fn evaluate_anchor(href_raw: &str, content: &str, out: &mut Vec<ScamIndicator>) {
    let href = decode_entities(href_raw);
    let href = href.trim();
    if !is_http_url(href) {
        return; // mailto: and anything without a web scheme carry no host.
    }
    let Some(href_host) = url_host(href) else {
        return;
    };

    if is_ip_literal(&href_host) {
        push_unique(
            out,
            ScamIndicator {
                kind: ScamIndicatorKind::IpAddressHost,
                detail: format!("This link points to a numeric IP address ({href_host})."),
            },
        );
    }

    let visible = strip_tags_and_decode(content);
    if let Some(text_host) = parse_text_host(&visible)
        && registrable_domain(&text_host) != registrable_domain(&href_host)
    {
        push_unique(
            out,
            ScamIndicator {
                kind: ScamIndicatorKind::DeceptiveLink,
                detail: format!(
                    "Link text shows \"{text_host}\" but the link goes to \"{href_host}\"."
                ),
            },
        );
    }
}

/// Adds `indicator` unless an identical one is already present.
fn push_unique(out: &mut Vec<ScamIndicator>, indicator: ScamIndicator) {
    if !out.contains(&indicator) {
        out.push(indicator);
    }
}

/// Extracts the value of attribute `name` from an opening tag, honoring
/// double/single quotes and the rare unquoted form. Case-insensitive name.
fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let idx = from + rel;
        let before_ok = idx == 0
            || matches!(
                bytes[idx - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'/'
            );
        let mut k = idx + name.len();
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if before_ok && bytes.get(k) == Some(&b'=') {
            k += 1;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            return read_attr_value(tag, k);
        }
        from = idx + name.len();
    }
    None
}

/// Reads an attribute value starting at byte offset `start` within `tag`.
fn read_attr_value(tag: &str, start: usize) -> Option<String> {
    let bytes = tag.as_bytes();
    match bytes.get(start) {
        Some(&q) if q == b'"' || q == b'\'' => {
            let value_start = start + 1;
            let end_rel = tag[value_start..].find(q as char)?;
            Some(tag[value_start..value_start + end_rel].to_owned())
        }
        Some(_) => {
            let end_rel = tag[start..]
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(tag.len() - start);
            Some(tag[start..start + end_rel].to_owned())
        }
        None => None,
    }
}

/// Removes inner tags from anchor content and decodes entities, yielding the
/// human-visible text.
fn strip_tags_and_decode(content: &str) -> String {
    let mut text = String::with_capacity(content.len());
    let mut depth = 0usize;
    for ch in content.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            c if depth == 0 => text.push(c),
            _ => {}
        }
    }
    decode_entities(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_quoted_href_case_insensitively() {
        assert_eq!(
            extract_attr("<a HREF=\"https://x/y\" rel=\"noopener\">", "href").as_deref(),
            Some("https://x/y")
        );
        assert_eq!(extract_attr("<a name=\"top\">", "href"), None);
    }

    #[test]
    fn strips_inner_markup_from_visible_text() {
        assert_eq!(strip_tags_and_decode("<b>pay</b>pal&#46;com"), "paypal.com");
    }
}
