//! Minimal redaction for parser warnings.
//!
//! The parser never echoes a raw address into a warning note. These helpers
//! keep only enough structure to be diagnostic (roughly "an address in the
//! To header was malformed") while masking the value itself, so a bad header
//! can never leak verbatim into a log stream.

/// Replacement used when nothing may be revealed.
const MASK: &str = "***";

/// Masks an address-shaped string for inclusion in a warning note.
///
/// Keeps only the first character of the local part and the top-level label
/// of the domain, masking everything else. Input that is not shaped like an
/// address collapses to `***`.
pub(crate) fn redact_address(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.split_once('@') {
        Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
            let head = local.chars().next().unwrap_or('*');
            match domain.rsplit_once('.') {
                Some((_, tld)) if !tld.is_empty() => format!("{head}{MASK}@{MASK}.{tld}"),
                _ => format!("{head}{MASK}@{MASK}"),
            }
        }
        _ => MASK.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_local_part_and_domain_but_keeps_tld() {
        assert_eq!(
            redact_address("ada.lovelace@mail.example.org"),
            "a***@***.org"
        );
    }

    #[test]
    fn masks_domain_without_dot() {
        assert_eq!(redact_address("user@nodot"), "u***@***");
    }

    #[test]
    fn non_address_input_is_fully_masked() {
        assert_eq!(redact_address("not-an-address"), "***");
    }

    #[test]
    fn never_echoes_the_full_input() {
        let raw = "secret.local@hidden.example.net";
        let redacted = redact_address(raw);
        assert!(!redacted.contains("secret.local"));
        assert!(!redacted.contains("hidden"));
    }
}
