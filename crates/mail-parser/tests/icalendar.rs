//! Tests for the iCalendar (iMIP) subset: parsing a `VEVENT` (fields, folded
//! lines, timezones, METHOD variants, CANCEL), extracting an invitation from a
//! parsed message, formatting the "when" line, and emitting a `METHOD:REPLY`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_parser::{
    CalDateTime, CalendarEvent, CalendarMethod, ReplyParams, TzSpec, build_reply,
    extract_invitation, format_when, parse_calendar, parse_message,
};

/// A folded, escaped REQUEST with a named timezone, two attendees, and an agenda.
const REQUEST: &str = concat!(
    "BEGIN:VCALENDAR\r\n",
    "PRODID:-//Example//EN\r\n",
    "VERSION:2.0\r\n",
    "METHOD:REQUEST\r\n",
    "BEGIN:VEVENT\r\n",
    "UID:abc-123@example.org\r\n",
    "SEQUENCE:2\r\n",
    "SUMMARY:Quarterly plan\\, review\r\n",
    "LOCATION:Room 5\r\n",
    "DESCRIPTION:Line one\\nLine two\r\n",
    "DTSTART;TZID=Europe/Berlin:20260720T130000\r\n",
    "DTEND;TZID=Europe/Berlin:20260720T140000\r\n",
    "ORGANIZER;CN=\"Ada Lovelace\":mailto:ada@example.org\r\n",
    "ATTENDEE;CN=Klaus;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:klaus@theferbers.eu\r\n",
    "ATTENDEE;CN=Bob:MAILTO:bob@example.org\r\n",
    "END:VEVENT\r\n",
    "END:VCALENDAR\r\n",
);

fn calendar_object(prop_lines: &str) -> String {
    format!("BEGIN:VCALENDAR\r\n{prop_lines}END:VCALENDAR\r\n")
}

#[test]
fn parses_request_vevent_fields() {
    let invite = parse_calendar(REQUEST).expect("an invitation");
    assert_eq!(invite.method, CalendarMethod::Request);
    let event = &invite.event;
    assert_eq!(event.uid, "abc-123@example.org");
    assert_eq!(event.sequence, 2);
    // TEXT escaping: `\,` decodes to a comma, `\n` to a newline.
    assert_eq!(event.summary.as_deref(), Some("Quarterly plan, review"));
    assert_eq!(event.description.as_deref(), Some("Line one\nLine two"));
    assert_eq!(event.location.as_deref(), Some("Room 5"));
    let organizer = event.organizer.as_ref().unwrap();
    assert_eq!(organizer.email, "ada@example.org");
    assert_eq!(organizer.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(event.attendees.len(), 2);
    assert_eq!(event.attendees[0].email, "klaus@theferbers.eu");
    assert_eq!(event.attendees[0].name.as_deref(), Some("Klaus"));
    assert_eq!(event.attendees[0].partstat.as_deref(), Some("NEEDS-ACTION"));
    // A case-varied MAILTO: scheme is still stripped.
    assert_eq!(event.attendees[1].email, "bob@example.org");
}

#[test]
fn parses_timezone_and_utc_datetimes() {
    let start = parse_calendar(REQUEST).unwrap().event.dtstart.unwrap();
    assert_eq!((start.year, start.month, start.day), (2026, 7, 20));
    assert_eq!((start.hour, start.minute), (13, 0));
    assert_eq!(start.tz, TzSpec::Named("Europe/Berlin".to_owned()));
    assert!(!start.date_only);

    let utc = calendar_object(
        "METHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:z\r\nDTSTART:20260720T110000Z\r\nEND:VEVENT\r\n",
    );
    let start = parse_calendar(&utc).unwrap().event.dtstart.unwrap();
    assert_eq!(start.tz, TzSpec::Utc);
    // 2026-07-20 11:00:00 UTC (exact for UTC).
    assert_eq!(start.to_unix_seconds(), 1_784_545_200);
}

#[test]
fn parses_all_day_date_value() {
    let all_day = calendar_object(
        "METHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:d\r\nDTSTART;VALUE=DATE:20260720\r\nEND:VEVENT\r\n",
    );
    let start = parse_calendar(&all_day).unwrap().event.dtstart.unwrap();
    assert!(start.date_only);
    assert_eq!((start.hour, start.minute, start.second), (0, 0, 0));
    assert_eq!(start.tz, TzSpec::Floating);
}

#[test]
fn unfolds_continuation_lines() {
    let folded = calendar_object(concat!(
        "METHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:f\r\n",
        "SUMMARY:This is a very long summary that the sender\r\n",
        "  folded across two lines\r\n",
        "END:VEVENT\r\n",
    ));
    let event = parse_calendar(&folded).unwrap().event;
    assert_eq!(
        event.summary.as_deref(),
        Some("This is a very long summary that the sender folded across two lines"),
    );
}

#[test]
fn recognizes_method_variants() {
    for (token, expected) in [
        ("REQUEST", CalendarMethod::Request),
        ("REPLY", CalendarMethod::Reply),
        ("CANCEL", CalendarMethod::Cancel),
        ("PUBLISH", CalendarMethod::Publish),
    ] {
        let ics = calendar_object(&format!(
            "METHOD:{token}\r\nBEGIN:VEVENT\r\nUID:m\r\nEND:VEVENT\r\n"
        ));
        assert_eq!(parse_calendar(&ics).unwrap().method, expected);
    }
}

#[test]
fn cancel_status_is_detected() {
    let cancel = calendar_object(
        "METHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:c\r\nSTATUS:CANCELLED\r\nSEQUENCE:3\r\nEND:VEVENT\r\n",
    );
    let invite = parse_calendar(&cancel).unwrap();
    assert_eq!(invite.method, CalendarMethod::Cancel);
    assert!(invite.event.is_cancelled());
}

#[test]
fn ignores_valarm_sub_component_properties() {
    // A VALARM inside the VEVENT carries its own DESCRIPTION/ATTENDEE that must
    // not leak into the event.
    let with_alarm = calendar_object(concat!(
        "METHOD:REQUEST\r\nBEGIN:VEVENT\r\n",
        "UID:a\r\nSUMMARY:Real summary\r\n",
        "BEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\n",
        "ATTENDEE:mailto:alarm@example.org\r\nEND:VALARM\r\n",
        "END:VEVENT\r\n",
    ));
    let event = parse_calendar(&with_alarm).unwrap().event;
    assert_eq!(event.summary.as_deref(), Some("Real summary"));
    assert!(
        event.description.is_none(),
        "VALARM description must not leak"
    );
    assert!(event.attendees.is_empty(), "VALARM attendee must not leak");
}

#[test]
fn a_calendar_without_method_is_not_an_invitation() {
    let no_method = calendar_object("BEGIN:VEVENT\r\nUID:n\r\nEND:VEVENT\r\n");
    assert!(parse_calendar(&no_method).is_none());
}

#[test]
fn non_calendar_text_yields_none() {
    assert!(parse_calendar("just some plain text").is_none());
}

#[test]
fn extract_invitation_finds_the_calendar_part_of_a_message() {
    // A multipart message whose second part is the invitation.
    let raw = format!(
        concat!(
            "From: Ada <ada@example.org>\r\nTo: Klaus <klaus@theferbers.eu>\r\n",
            "Subject: Invite\r\nMIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n",
            "--b\r\nContent-Type: text/plain\r\n\r\nYou are invited.\r\n",
            "--b\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\n{}",
            "--b--\r\n",
        ),
        REQUEST,
    );
    let parsed = parse_message(raw.as_bytes()).unwrap();
    let invite = extract_invitation(&parsed).expect("the calendar part is extracted");
    assert_eq!(invite.method, CalendarMethod::Request);
    assert_eq!(invite.event.uid, "abc-123@example.org");
}

#[test]
fn a_plain_message_extracts_no_invitation() {
    let raw = concat!(
        "From: Ada <ada@example.org>\r\nSubject: Hi\r\n",
        "Content-Type: text/plain\r\n\r\nNo calendar here.\r\n",
    );
    let parsed = parse_message(raw.as_bytes()).unwrap();
    assert!(extract_invitation(&parsed).is_none());
}

fn dt(hour: u32, minute: u32, tz: TzSpec) -> CalDateTime {
    CalDateTime {
        year: 2026,
        month: 7,
        day: 20,
        hour,
        minute,
        second: 0,
        date_only: false,
        tz,
    }
}

#[test]
fn utc_to_unix_is_exact() {
    assert_eq!(dt(11, 0, TzSpec::Utc).to_unix_seconds(), 1_784_545_200);
}

#[test]
fn format_when_renders_range_and_zone() {
    let start = dt(13, 0, TzSpec::Named("Europe/Berlin".to_owned()));
    let end = dt(14, 30, TzSpec::Named("Europe/Berlin".to_owned()));
    assert_eq!(
        format_when(Some(&start), Some(&end)).as_deref(),
        Some("20 Jul 2026, 13:00 – 14:30 (Europe/Berlin)"),
    );
}

#[test]
fn format_when_all_day_has_no_clock() {
    let start = CalDateTime {
        date_only: true,
        tz: TzSpec::Floating,
        ..dt(0, 0, TzSpec::Floating)
    };
    assert_eq!(
        format_when(Some(&start), None).as_deref(),
        Some("20 Jul 2026")
    );
}

#[test]
fn attendee_matching_is_case_insensitive() {
    let event = parse_calendar(REQUEST).unwrap().event;
    let mine = vec!["KLAUS@theferbers.eu".to_owned()];
    assert_eq!(
        event.attendee_matching(&mine).map(|a| a.email.as_str()),
        Some("klaus@theferbers.eu"),
    );
}

/// The event from [`REQUEST`], for reply-emission tests.
fn request_event() -> CalendarEvent {
    parse_calendar(REQUEST).unwrap().event
}

fn accept_params() -> ReplyParams {
    ReplyParams {
        responder_email: "klaus@theferbers.eu".to_owned(),
        responder_name: Some("Klaus Ferber".to_owned()),
        partstat: "ACCEPTED".to_owned(),
        dtstamp_unix: 1_784_545_200,
    }
}

#[test]
fn reply_carries_method_partstat_and_identity() {
    let reply = build_reply(&request_event(), &accept_params());
    assert!(reply.contains("METHOD:REPLY"));
    assert!(reply.contains("UID:abc-123@example.org"));
    assert!(reply.contains("SEQUENCE:2"));
    // DTSTAMP is the UTC stamp for the passed instant (covers the date math).
    assert!(reply.contains("DTSTAMP:20260720T110000Z"));
    assert!(reply.contains("PARTSTAT=ACCEPTED"));
    assert!(reply.contains("mailto:klaus@theferbers.eu"));
    assert!(reply.contains("CN=\"Klaus Ferber\""));
    assert!(reply.contains("ORGANIZER;CN=\"Ada Lovelace\":mailto:ada@example.org"));
    // The start/end are emitted as self-contained UTC, never echoed with a bare
    // TZID (a TZID without its VTIMEZONE is invalid, ambiguous iCalendar).
    assert!(reply.contains("DTSTART:20260720T130000Z"), "{reply}");
    assert!(reply.contains("DTEND:20260720T140000Z"), "{reply}");
    assert!(
        !reply.contains("TZID"),
        "the reply must not carry a dangling TZID: {reply}"
    );
    assert!(reply.contains("\r\n"), "reply uses CRLF line endings");
}

#[test]
fn reply_has_exactly_one_attendee() {
    let reply = build_reply(&request_event(), &accept_params());
    assert_eq!(reply.matches("ATTENDEE").count(), 1);
}

#[test]
fn reply_round_trips_through_the_parser() {
    let reply = build_reply(&request_event(), &accept_params());
    let parsed = parse_calendar(&reply).expect("reply parses back");
    assert_eq!(parsed.method, CalendarMethod::Reply);
    assert_eq!(parsed.event.uid, "abc-123@example.org");
    assert_eq!(parsed.event.attendees.len(), 1);
    assert_eq!(
        parsed.event.attendees[0].partstat.as_deref(),
        Some("ACCEPTED")
    );
}

#[test]
fn declined_partstat_is_written() {
    let mut params = accept_params();
    params.partstat = "DECLINED".to_owned();
    let reply = build_reply(&request_event(), &params);
    assert!(reply.contains("PARTSTAT=DECLINED"));
}

#[test]
fn long_lines_are_folded_to_75_octets() {
    let mut event = request_event();
    event.summary = Some("x".repeat(200));
    let reply = build_reply(&event, &accept_params());
    for physical in reply.split("\r\n") {
        assert!(
            physical.len() <= 75,
            "physical line exceeds 75 octets ({}): {physical}",
            physical.len(),
        );
    }
}
