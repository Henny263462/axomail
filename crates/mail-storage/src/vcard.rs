//! Dependency-free vCard 3.0 / 4.0 reading and writing for the address book.
//!
//! [`parse_vcards`] reads an exported `.vcf` (one or many cards), and
//! [`emit_vcards`] writes contacts back out. The two round-trip: emitting a
//! parsed card and re-parsing yields the same [`VCard`]. The type is a faithful,
//! storage-agnostic projection of the subset the address book uses — formatted
//! name, addresses (with a label and a "preferred" flag), and a note — so the
//! command layer maps between it and the repository's `NewContact` / `Contact`.
//!
//! Handled per RFC 6350 (4.0) and RFC 2426 (3.0): line folding/unfolding,
//! property parameters (both `TYPE=work` and legacy bare `TYPE`), `PREF` in its
//! 3.0 (`TYPE=PREF`) and 4.0 (`PREF=1`) forms, and value escaping
//! (`\n \, \; \\`). Property groups (`item1.EMAIL`) and unknown properties are
//! tolerated. Parsing never fails on malformed content: a card missing a
//! `BEGIN`/`END` pair or an address is simply skipped or left empty, so a
//! partly-broken export still imports what it can.

use axomail_core::MailError;

/// The vCard version to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VCardVersion {
    /// vCard 3.0 (RFC 2426) — the most widely interoperable export.
    V3,
    /// vCard 4.0 (RFC 6350).
    V4,
}

impl VCardVersion {
    fn as_str(self) -> &'static str {
        match self {
            Self::V3 => "3.0",
            Self::V4 => "4.0",
        }
    }
}

/// One address on a vCard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VCardEmail {
    /// The raw address text (not validated here; the caller validates).
    pub address: String,
    /// Presentation label derived from the `TYPE` parameter ("home", "work",
    /// ...); empty when none.
    pub label: String,
    /// Whether this address is marked preferred (`PREF`).
    pub preferred: bool,
}

/// A parsed or to-be-emitted vCard, limited to the address-book subset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VCard {
    /// Formatted display name (`FN`).
    pub formatted_name: String,
    /// Addresses (`EMAIL`), in file order.
    pub emails: Vec<VCardEmail>,
    /// Free-form note (`NOTE`); empty when none.
    pub note: String,
}

/// Parses zero or more vCards from exported text.
///
/// # Errors
///
/// Returns [`MailError::Parse`] only if the input is not valid UTF-8-shaped
/// text this reader can scan; malformed individual cards are skipped, not
/// errored, so a partial export still yields its good cards.
pub fn parse_vcards(input: &str) -> Result<Vec<VCard>, MailError> {
    let logical = unfold(input);
    let mut cards = Vec::new();
    let mut current: Option<VCard> = None;
    for line in &logical {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VCARD" {
            current = Some(VCard::default());
        } else if upper == "END:VCARD" {
            if let Some(card) = current.take() {
                cards.push(card);
            }
        } else if let Some(card) = current.as_mut() {
            apply_property(card, line);
        }
    }
    Ok(cards)
}

/// Serializes contacts as a vCard document (CRLF-terminated, folded at 75
/// octets), one `VCARD` per contact.
#[must_use]
pub fn emit_vcards(cards: &[VCard], version: VCardVersion) -> String {
    let mut out = String::new();
    for card in cards {
        out.push_str(&emit_one(card, version));
    }
    out
}

/// Emits a single card.
fn emit_one(card: &VCard, version: VCardVersion) -> String {
    let mut lines = vec![
        "BEGIN:VCARD".to_owned(),
        format!("VERSION:{}", version.as_str()),
        format!("FN:{}", escape_value(&card.formatted_name)),
    ];
    for email in &card.emails {
        lines.push(email_line(email, version));
    }
    if !card.note.is_empty() {
        lines.push(format!("NOTE:{}", escape_value(&card.note)));
    }
    lines.push("END:VCARD".to_owned());

    let mut out = String::new();
    for line in lines {
        out.push_str(&fold(&line));
        out.push_str("\r\n");
    }
    out
}

/// Builds an `EMAIL` property line with version-appropriate parameters.
fn email_line(email: &VCardEmail, version: VCardVersion) -> String {
    let mut params = String::new();
    let type_value = if email.label.is_empty() {
        None
    } else {
        Some(email.label.as_str())
    };
    match version {
        VCardVersion::V3 => {
            let mut types: Vec<&str> = Vec::new();
            if let Some(label) = type_value {
                types.push(label);
            }
            if email.preferred {
                types.push("PREF");
            }
            if !types.is_empty() {
                params.push_str(";TYPE=");
                params.push_str(&types.join(","));
            }
        }
        VCardVersion::V4 => {
            if let Some(label) = type_value {
                params.push_str(";TYPE=");
                params.push_str(label);
            }
            if email.preferred {
                params.push_str(";PREF=1");
            }
        }
    }
    format!("EMAIL{}:{}", params, escape_value(&email.address))
}

/// Applies one property line to the card under construction.
fn apply_property(card: &mut VCard, line: &str) {
    let Some((head, value)) = split_once_unquoted(line, ':') else {
        return;
    };
    let (name, params) = parse_head(head);
    let value = unescape_value(value);
    match name.as_str() {
        "FN" => card.formatted_name = value,
        "NOTE" => card.note = value,
        "EMAIL" if !value.trim().is_empty() => {
            card.emails.push(email_from(value, &params));
        }
        // N supplies a name only when FN has not (structured: Family;Given;...).
        "N" if card.formatted_name.is_empty() => {
            card.formatted_name = name_from_structured(&value);
        }
        _ => {}
    }
}

/// Builds a [`VCardEmail`] from an address and its parsed parameters.
fn email_from(address: String, params: &[(String, Vec<String>)]) -> VCardEmail {
    let mut label = String::new();
    let mut preferred = false;
    for (key, values) in params {
        match key.as_str() {
            "TYPE" => {
                for value in values {
                    if value.eq_ignore_ascii_case("pref") {
                        preferred = true;
                    } else if !value.eq_ignore_ascii_case("internet")
                        && !value.eq_ignore_ascii_case("email")
                        && label.is_empty()
                    {
                        label = value.to_ascii_lowercase();
                    }
                }
            }
            "PREF" => preferred = true,
            _ => {}
        }
    }
    VCardEmail {
        address,
        label,
        preferred,
    }
}

/// Splits a property head into its uppercased name (group prefix dropped) and
/// its parameters.
fn parse_head(head: &str) -> (String, Vec<(String, Vec<String>)>) {
    let mut parts = split_unquoted(head, ';');
    let raw_name = parts.next().unwrap_or_default();
    // Drop a `group.` prefix (e.g. `item1.EMAIL`).
    let name = raw_name
        .rsplit('.')
        .next()
        .unwrap_or(raw_name)
        .trim()
        .to_ascii_uppercase();
    let mut params = Vec::new();
    for part in parts {
        if let Some((key, value)) = part.split_once('=') {
            let values = split_unquoted(value, ',')
                .map(strip_quotes)
                .filter(|value| !value.is_empty())
                .collect();
            params.push((key.trim().to_ascii_uppercase(), values));
        } else {
            // Legacy bare parameter (vCard 3.0 `EMAIL;HOME:`) is a TYPE value.
            let bare = strip_quotes(part);
            if !bare.is_empty() {
                params.push(("TYPE".to_owned(), vec![bare]));
            }
        }
    }
    (name, params)
}

/// Extracts a display name from a structured `N` value
/// (`Family;Given;Additional;Prefix;Suffix`) as `Given Family`.
fn name_from_structured(value: &str) -> String {
    let fields: Vec<&str> = value.split(';').collect();
    let family = fields.first().copied().unwrap_or("").trim();
    let given = fields.get(1).copied().unwrap_or("").trim();
    match (given.is_empty(), family.is_empty()) {
        (false, false) => format!("{given} {family}"),
        (false, true) => given.to_owned(),
        (true, false) => family.to_owned(),
        (true, true) => String::new(),
    }
}

/// Unfolds RFC 6350 folded lines: a line beginning with a space or tab
/// continues the previous one (its leading whitespace removed). Blank lines are
/// dropped.
fn unfold(input: &str) -> Vec<String> {
    let mut logical: Vec<String> = Vec::new();
    for raw in input.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix([' ', '\t'])
            && let Some(last) = logical.last_mut()
        {
            last.push_str(rest);
            continue;
        }
        if !line.is_empty() {
            logical.push(line.to_owned());
        }
    }
    logical
}

/// Folds a logical line to 75 octets per continuation (RFC 6350 §3.2), breaking
/// on character boundaries and prefixing continuations with a single space.
fn fold(line: &str) -> String {
    const LIMIT: usize = 75;
    if line.len() <= LIMIT {
        return line.to_owned();
    }
    let mut out = String::new();
    // `width` is the octet length of the current physical line. A continuation
    // line's leading space is counted in `width`, so the same 75-octet ceiling
    // applies uniformly to every physical line.
    let mut width = 0usize;
    for ch in line.chars() {
        if width + ch.len_utf8() > LIMIT {
            out.push_str("\r\n ");
            width = 1; // the leading space
        }
        out.push(ch);
        width += ch.len_utf8();
    }
    out
}

/// Escapes a value for emission (`\ ; , newline`).
fn escape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Reverses [`escape_value`]: `\n \N \, \; \\` become their literals.
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Removes surrounding double quotes from a parameter value.
fn strip_quotes(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_owned()
}

/// Splits at the first unquoted occurrence of `sep`, returning head and tail.
fn split_once_unquoted(input: &str, sep: char) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (index, ch) in input.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == sep && !quoted {
            return Some((&input[..index], &input[index + ch.len_utf8()..]));
        }
    }
    None
}

/// Iterates the `sep`-separated segments of `input`, ignoring separators inside
/// double quotes.
fn split_unquoted(input: &str, sep: char) -> impl Iterator<Item = &str> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut quoted = false;
    for (index, ch) in input.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == sep && !quoted {
            segments.push(&input[start..index]);
            start = index + ch.len_utf8();
        }
    }
    segments.push(&input[start..]);
    segments.into_iter()
}
