//! Allowlist policy for sanitizing untrusted HTML mail (ADR-0007).
//!
//! This module owns the static allowlist that shapes the [`ammonia::Builder`]
//! driven by [`crate::html`]. Everything here is a pure description of what
//! *may* survive; the per-message attribute filter in [`crate::html`] narrows
//! it further (blocking remote images, restricting link schemes, counting
//! blocked resources). Keeping the two apart makes the policy auditable in
//! isolation and lets the filter carry the only mutable per-call state.
//!
//! The policy is intentionally conservative: an unknown tag or attribute is
//! dropped, not passed through. `style` and `class` are absent by design, so
//! CSS never survives (ADR-0007: viewport-escaping styles and `url()`
//! exfiltration are removed wholesale rather than filtered property by
//! property, per "if in doubt, strip").

use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};

/// `rel` value forced onto every surviving hyperlink so that a bypass of the
/// frontend's link interception still cannot reach `window.opener`.
pub(crate) const LINK_REL: &str = "noopener noreferrer";

/// URL schemes ammonia may keep on URL-bearing attributes.
///
/// This set is deliberately wider than what ultimately survives. The
/// attribute filter in [`crate::html`] tightens it per attribute: only
/// `cid:` images and only `http`/`https`/`mailto` links are retained.
/// `data` is admitted here *solely* so the filter can observe and count
/// blocked `data:` images; ammonia would otherwise strip them before the
/// filter ever runs (it scheme-checks URL attributes against this set
/// first). `javascript:`, `vbscript:`, and every other scheme are absent
/// and therefore removed by ammonia outright.
fn url_schemes() -> HashSet<&'static str> {
    ["http", "https", "mailto", "cid", "data"]
        .into_iter()
        .collect()
}

/// Tags allowed to appear in sanitized output.
///
/// Structural, textual, list, table, link, and image elements only. Any tag
/// carrying script, style, form, embedding, or foreign-content semantics is
/// absent (and, where it can wrap text, listed in [`clean_content_tags`] so
/// its subtree is removed rather than flattened to visible text).
fn allowed_tags() -> HashSet<&'static str> {
    [
        // Flow and text-level semantics.
        "p",
        "div",
        "span",
        "br",
        "hr",
        "pre",
        "blockquote",
        "code",
        "q",
        "cite",
        "abbr",
        "address",
        "wbr",
        "mark",
        "small",
        "sub",
        "sup",
        "b",
        "strong",
        "i",
        "em",
        "u",
        "s",
        "strike",
        "del",
        "ins",
        "kbd",
        "samp",
        "var",
        "tt",
        "dfn",
        "bdi",
        "bdo",
        "ruby",
        "rt",
        "rp",
        "time",
        "center",
        "font",
        "big",
        // Headings.
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        // Lists.
        "ul",
        "ol",
        "li",
        "dl",
        "dt",
        "dd",
        // Tables.
        "table",
        "caption",
        "colgroup",
        "col",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "td",
        "th",
        // Links, images, figures.
        "a",
        "img",
        "figure",
        "figcaption",
    ]
    .into_iter()
    .collect()
}

/// Tags whose entire subtree is removed rather than flattened to text.
///
/// For these, dropping only the tag would leak executable code, CSS, field
/// defaults, or foreign markup as visible text. Removing the subtree keeps
/// the rendered message clean and prevents raw content from surfacing.
fn clean_content_tags() -> HashSet<&'static str> {
    [
        "script", "style", "iframe", "object", "embed", "applet", "svg", "math", "noscript",
        "template", "title", "head", "textarea", "frame", "frameset", "noframes",
    ]
    .into_iter()
    .collect()
}

/// Attributes permitted on every allowed tag.
///
/// Purely descriptive, non-URL, non-scripting attributes. `style`, `class`,
/// and `id` are excluded on purpose.
fn generic_attributes() -> HashSet<&'static str> {
    ["title", "dir", "lang"].into_iter().collect()
}

/// Per-tag attribute allowlist.
///
/// Only presentational, non-executable attributes are kept. The single
/// URL-bearing entries are `a@href` and `img@src`/`img@srcset`, all of which
/// the attribute filter in [`crate::html`] inspects further. `cite`,
/// `formaction`, `ping`, `poster`, and similar URL sinks are deliberately
/// omitted so no unfiltered URL attribute can reach the output.
fn tag_attributes() -> HashMap<&'static str, HashSet<&'static str>> {
    let cell: HashSet<&'static str> = [
        "align", "valign", "bgcolor", "colspan", "rowspan", "width", "height", "nowrap", "headers",
        "scope",
    ]
    .into_iter()
    .collect();

    let mut map: HashMap<&'static str, HashSet<&'static str>> = HashMap::new();
    map.insert("a", ["href", "name"].into_iter().collect());
    map.insert(
        "img",
        ["src", "srcset", "alt", "width", "height", "align"]
            .into_iter()
            .collect(),
    );
    map.insert("font", ["color", "face", "size"].into_iter().collect());
    map.insert(
        "table",
        [
            "align",
            "bgcolor",
            "border",
            "cellpadding",
            "cellspacing",
            "width",
            "summary",
        ]
        .into_iter()
        .collect(),
    );
    map.insert("col", ["align", "span", "width"].into_iter().collect());
    map.insert("colgroup", ["align", "span", "width"].into_iter().collect());
    map.insert("tr", ["align", "valign", "bgcolor"].into_iter().collect());
    map.insert("td", cell.clone());
    map.insert("th", cell);
    map.insert("tbody", ["align", "valign"].into_iter().collect());
    map.insert("thead", ["align", "valign"].into_iter().collect());
    map.insert("tfoot", ["align", "valign"].into_iter().collect());
    map.insert("ol", ["start", "type", "reversed"].into_iter().collect());
    map.insert("ul", ["type"].into_iter().collect());
    map.insert("li", ["value", "type"].into_iter().collect());
    map.insert("hr", ["align", "width", "size"].into_iter().collect());
    map.insert("bdo", ["dir"].into_iter().collect());
    map.insert("bdi", ["dir"].into_iter().collect());
    map.insert("time", ["datetime"].into_iter().collect());
    map.insert("del", ["datetime"].into_iter().collect());
    map.insert("ins", ["datetime"].into_iter().collect());
    map
}

/// Builds the message sanitization policy, minus the per-call attribute
/// filter (which [`crate::html`] installs because it carries mutable state).
///
/// Relative URLs are denied outright: a mail body has no trustworthy base,
/// so a bare `href="/path"` must not be resolved against the app origin.
pub(crate) fn message_policy() -> Builder<'static> {
    let mut builder = Builder::default();
    builder
        .tags(allowed_tags())
        .clean_content_tags(clean_content_tags())
        .tag_attributes(tag_attributes())
        .generic_attributes(generic_attributes())
        .url_schemes(url_schemes())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some(LINK_REL))
        .strip_comments(true);
    builder
}
