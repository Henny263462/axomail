//! Integration tests for the vCard reader/writer: 3.0 and 4.0 parsing, the
//! emit→parse round-trip, multiple addresses with labels/preferred, line
//! folding/unfolding, value escaping, and lenient handling of malformed input.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_storage::{VCard, VCardEmail, VCardVersion, emit_vcards, parse_vcards};

fn email(address: &str, label: &str, preferred: bool) -> VCardEmail {
    VCardEmail {
        address: address.to_owned(),
        label: label.to_owned(),
        preferred,
    }
}

#[test]
fn parses_a_vcard_3_with_typed_and_preferred_addresses() {
    let text = "BEGIN:VCARD\r\n\
                VERSION:3.0\r\n\
                FN:Ada Lovelace\r\n\
                EMAIL;TYPE=HOME:ada@home.example.org\r\n\
                EMAIL;TYPE=WORK,PREF:ada@work.example.org\r\n\
                NOTE:First programmer\r\n\
                END:VCARD\r\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.formatted_name, "Ada Lovelace");
    assert_eq!(card.note, "First programmer");
    assert_eq!(card.emails.len(), 2);
    assert_eq!(card.emails[0], email("ada@home.example.org", "home", false));
    assert_eq!(card.emails[1], email("ada@work.example.org", "work", true));
}

#[test]
fn parses_vcard_4_pref_parameter_and_bare_type() {
    let text = "BEGIN:VCARD\n\
                VERSION:4.0\n\
                FN:Grace Hopper\n\
                EMAIL;TYPE=work;PREF=1:grace@work.example.org\n\
                EMAIL;HOME:grace@home.example.org\n\
                END:VCARD\n";
    let cards = parse_vcards(text).unwrap();
    let card = &cards[0];
    assert_eq!(
        card.emails[0],
        email("grace@work.example.org", "work", true)
    );
    // Legacy bare `;HOME` is read as a TYPE value.
    assert_eq!(
        card.emails[1],
        email("grace@home.example.org", "home", false)
    );
}

#[test]
fn derives_name_from_structured_n_when_fn_absent() {
    let text = "BEGIN:VCARD\nVERSION:3.0\nN:Hopper;Grace;;;\nEMAIL:g@example.org\nEND:VCARD\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards[0].formatted_name, "Grace Hopper");
}

#[test]
fn unfolds_folded_lines() {
    // A note split across a fold boundary (continuation begins with a space).
    let text =
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:X\r\nNOTE:this is a long \r\n note\r\nEND:VCARD\r\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards[0].note, "this is a long note");
}

#[test]
fn unescapes_special_characters() {
    let text = "BEGIN:VCARD\nVERSION:3.0\nFN:Doe\\, John\nNOTE:line1\\nline2\\;still\nEMAIL:j@example.org\nEND:VCARD\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards[0].formatted_name, "Doe, John");
    assert_eq!(cards[0].note, "line1\nline2;still");
}

#[test]
fn emits_and_reparses_round_trip_v3() {
    let card = VCard {
        formatted_name: "Ada; Countess, of Lovelace".to_owned(),
        emails: vec![
            email("ada@work.example.org", "work", true),
            email("ada@home.example.org", "home", false),
        ],
        note: "multi\nline note".to_owned(),
    };
    let text = emit_vcards(std::slice::from_ref(&card), VCardVersion::V3);
    assert!(text.contains("VERSION:3.0"));
    let reparsed = parse_vcards(&text).unwrap();
    assert_eq!(reparsed.len(), 1);
    assert_eq!(reparsed[0], card);
}

#[test]
fn emits_and_reparses_round_trip_v4() {
    let card = VCard {
        formatted_name: "Grace Hopper".to_owned(),
        emails: vec![email("grace@example.org", "work", true)],
        note: String::new(),
    };
    let text = emit_vcards(std::slice::from_ref(&card), VCardVersion::V4);
    assert!(text.contains("VERSION:4.0"));
    assert!(text.contains("PREF=1"));
    assert_eq!(parse_vcards(&text).unwrap()[0], card);
}

#[test]
fn round_trips_a_long_value_through_folding() {
    // A long address forces the emitter to fold; unfolding must restore it.
    let long_local = "a".repeat(120);
    let address = format!("{long_local}@example.org");
    let card = VCard {
        formatted_name: "Long".to_owned(),
        emails: vec![email(&address, "", false)],
        note: String::new(),
    };
    let text = emit_vcards(std::slice::from_ref(&card), VCardVersion::V3);
    // The emitted document folded (a CRLF + space appears mid-document).
    assert!(text.contains("\r\n "));
    let reparsed = parse_vcards(&text).unwrap();
    assert_eq!(reparsed[0].emails[0].address, address);
}

#[test]
fn parses_multiple_cards_and_skips_a_broken_one() {
    let text = "BEGIN:VCARD\nVERSION:3.0\nFN:One\nEMAIL:one@example.org\nEND:VCARD\n\
                BEGIN:VCARD\nVERSION:3.0\nFN:Two\nEMAIL:two@example.org\nEND:VCARD\n\
                FN:Orphan line outside any card\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].formatted_name, "One");
    assert_eq!(cards[1].formatted_name, "Two");
}

#[test]
fn tolerates_group_prefixes_and_unknown_properties() {
    let text = "BEGIN:VCARD\nVERSION:3.0\nFN:Item\nitem1.EMAIL;TYPE=WORK:x@example.org\nX-CUSTOM:ignored\nEND:VCARD\n";
    let cards = parse_vcards(text).unwrap();
    assert_eq!(cards[0].emails.len(), 1);
    assert_eq!(cards[0].emails[0], email("x@example.org", "work", false));
}

#[test]
fn empty_input_yields_no_cards() {
    assert!(parse_vcards("").unwrap().is_empty());
    assert!(parse_vcards("not a vcard at all\n").unwrap().is_empty());
}
