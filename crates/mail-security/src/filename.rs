//! Pure filename sanitization for attachments.
//!
//! The single entry point, [`sanitize_attachment_filename`], turns a
//! server-supplied attachment name into a value that is safe to use as a
//! single path component when writing to the attachment store. It performs
//! no I/O and makes no filesystem queries; it only transforms the string.

/// Name used when sanitization would otherwise yield an empty result.
const FALLBACK: &str = "attachment";

/// Maximum length of the returned name, in Unicode scalar values.
const MAX_LEN: usize = 150;

/// Maximum length of a preserved extension (including the leading dot).
const MAX_EXT_LEN: usize = 16;

/// Sanitizes an attachment filename into a safe single path component.
///
/// The result is guaranteed to be non-empty, to contain no path separators
/// or control characters, to be neither `.` nor `..`, to avoid
/// Windows-reserved device names, and to be at most 150 characters long
/// (preserving a reasonable extension where possible).
///
/// The transformation, in order:
/// - drops control characters and replaces path separators (`/`, `\`) and
///   the Windows-forbidden characters `<>:"|?*` with `_`;
/// - trims surrounding whitespace and trailing dots and spaces (which
///   Windows would otherwise strip, changing which file is addressed);
/// - prefixes `_` to reserved device names such as `CON`, `NUL`, `COM1`, or
///   `LPT9`, with or without an extension;
/// - truncates to 150 characters, keeping the extension when it is short;
/// - falls back to `attachment` if nothing usable remains (including the
///   bare `.` and `..` inputs).
#[must_use]
pub fn sanitize_attachment_filename(input: &str) -> String {
    let replaced: String = input.chars().filter_map(map_char).collect();
    let collapsed = collapse(&replaced);
    if collapsed.is_empty() {
        return FALLBACK.to_owned();
    }

    let guarded = guard_reserved(&collapsed);
    let limited = enforce_length(&guarded);
    let final_name = collapse(&limited);
    if final_name.is_empty() || final_name.bytes().all(|b| b == b'.') {
        return FALLBACK.to_owned();
    }
    final_name
}

/// Maps a single character: `None` drops it, `Some(c)` keeps or replaces it.
fn map_char(c: char) -> Option<char> {
    if matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*') {
        Some('_')
    } else if c.is_control() || is_format_control(c) {
        None
    } else {
        Some(c)
    }
}

/// Whether `c` is a Unicode bidirectional-override, zero-width, or other
/// invisible format/general-punctuation control that must not survive into a
/// saved filename.
///
/// These carry no printable width but reorder or hide surrounding text, so a
/// saved name like `photo` + `U+202E` + `gpj.exe` renders as `photoexe.jpg`
/// while being an executable — the classic right-to-left-override spoof. `char::
/// is_control` covers only C0/C1 controls (category Cc), so these format
/// codepoints (category Cf and friends) are stripped separately.
fn is_format_control(c: char) -> bool {
    matches!(c,
        '\u{061C}'                  // Arabic letter mark
        | '\u{200B}'..='\u{200F}'   // zero-width space/joiners, LRM, RLM
        | '\u{202A}'..='\u{202E}'   // bidi embeddings/overrides (LRE…RLO)
        | '\u{2060}'..='\u{2064}'   // word joiner, invisible operators
        | '\u{2066}'..='\u{206F}'   // bidi isolates + deprecated format chars
        | '\u{FEFF}'                // zero-width no-break space / BOM
        | '\u{FFF9}'..='\u{FFFB}'   // interlinear annotation controls
        | '\u{1D173}'..='\u{1D17A}' // musical-notation format controls
        | '\u{E0000}'..='\u{E007F}' // tag characters (invisible)
    )
}

/// Trims surrounding whitespace and any trailing dots or spaces.
fn collapse(s: &str) -> String {
    s.trim_matches(char::is_whitespace)
        .trim_end_matches(['.', ' '])
        .to_owned()
}

/// Prefixes `_` to a Windows-reserved device name, leaving others intact.
fn guard_reserved(name: &str) -> String {
    let stem = name.split('.').next().unwrap_or(name);
    if is_reserved_stem(stem) {
        format!("_{name}")
    } else {
        name.to_owned()
    }
}

/// Returns `true` for a reserved DOS device name (case-insensitive).
fn is_reserved_stem(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || is_numbered_device(&upper, "COM")
        || is_numbered_device(&upper, "LPT")
}

/// Matches `COM1`..`COM9` / `LPT1`..`LPT9` (digit `0` is not reserved).
fn is_numbered_device(upper: &str, prefix: &str) -> bool {
    match upper.strip_prefix(prefix) {
        Some(rest) => rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_digit() && b != b'0'),
        None => false,
    }
}

/// Truncates to [`MAX_LEN`] characters, preserving a short extension.
fn enforce_length(name: &str) -> String {
    if name.chars().count() <= MAX_LEN {
        return name.to_owned();
    }
    if let Some(dot) = name.rfind('.') {
        let ext = &name[dot..];
        let ext_len = ext.chars().count();
        if dot > 0 && ext_len > 1 && ext_len <= MAX_EXT_LEN {
            let stem: String = name[..dot].chars().take(MAX_LEN - ext_len).collect();
            return format!("{stem}{ext}");
        }
    }
    name.chars().take(MAX_LEN).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reserved_stem_detection_covers_boundaries() {
        assert!(is_reserved_stem("CON"));
        assert!(is_reserved_stem("com1"));
        assert!(is_reserved_stem("LPT9"));
        assert!(!is_reserved_stem("COM0"));
        assert!(!is_reserved_stem("COM10"));
        assert!(!is_reserved_stem("CONSOLE"));
        assert!(!is_reserved_stem(""));
    }

    #[test]
    fn extension_is_preserved_when_truncating() {
        let long = format!("{}.pdf", "a".repeat(400));
        let out = sanitize_attachment_filename(&long);
        assert!(out.ends_with(".pdf"));
        assert_eq!(out.chars().count(), MAX_LEN);
    }

    #[test]
    fn strips_bidi_and_zero_width_format_controls() {
        // A right-to-left override spoof: "photo" + U+202E + "gpj.exe" renders as
        // "photoexe.jpg" but is an executable. The control must be stripped.
        let out = sanitize_attachment_filename("photo\u{202E}gpj.exe");
        assert!(!out.contains('\u{202E}'), "RLO not stripped: {out:?}");
        assert_eq!(out, "photogpj.exe");
        // Zero-width space and BOM are invisible format controls too.
        assert_eq!(
            sanitize_attachment_filename("in\u{200B}voice\u{FEFF}.pdf"),
            "invoice.pdf"
        );
    }
}
