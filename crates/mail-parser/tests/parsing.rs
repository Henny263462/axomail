//! Integration tests for the MIME parsing layer.
//!
//! Messages are crafted inline with LF line endings and normalized to CRLF by
//! [`msg`], which keeps the corpus readable while feeding the parser canonical
//! RFC 5322 line breaks.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axomail_parser::{ParsedMessage, parse_message};

/// Normalizes a raw message to CRLF line endings and returns its bytes.
fn msg(raw: &str) -> Vec<u8> {
    raw.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes()
}

/// Parses a crafted message, asserting success.
fn parse(raw: &str) -> ParsedMessage {
    parse_message(&msg(raw)).expect("message should parse")
}

#[test]
fn parses_disposition_notification_to_request() {
    let parsed = parse(
        "From: Ada Lovelace <ada@example.org>\n\
         To: Charles <charles@example.com>\n\
         Subject: Please confirm\n\
         Disposition-Notification-To: Ada <ada@example.org>\n\
         \n\
         Did you read this?\n",
    );
    assert_eq!(parsed.disposition_notification_to.len(), 1);
    assert_eq!(
        parsed.disposition_notification_to[0].email.as_str(),
        "ada@example.org"
    );
    assert!(!parsed.mailing_list);
}

#[test]
fn no_receipt_request_when_header_absent() {
    let parsed = parse(
        "From: Ada <ada@example.org>\n\
         To: Charles <charles@example.com>\n\
         Subject: Hello\n\
         \n\
         Plain message.\n",
    );
    assert!(parsed.disposition_notification_to.is_empty());
}

#[test]
fn detects_mailing_list_from_list_id_header() {
    let parsed = parse(
        "From: list@example.org\n\
         To: subscriber@example.com\n\
         Subject: Digest\n\
         List-Id: Example Discussion <discuss.example.org>\n\
         Disposition-Notification-To: list@example.org\n\
         \n\
         Weekly digest.\n",
    );
    assert!(parsed.mailing_list);
    assert_eq!(parsed.disposition_notification_to.len(), 1);
}

#[test]
fn detects_mailing_list_from_precedence_bulk() {
    let parsed = parse(
        "From: noreply@example.org\n\
         To: subscriber@example.com\n\
         Subject: Notice\n\
         Precedence: bulk\n\
         \n\
         Automated notice.\n",
    );
    assert!(parsed.mailing_list);
}

#[test]
fn simple_plain_extracts_all_envelope_fields() {
    let parsed = parse(
        "From: Ada Lovelace <ada@example.org>\n\
         To: Charles <charles@example.com>\n\
         Subject: Hello\n\
         Date: Tue, 1 Jul 2025 09:30:00 +0000\n\
         Message-ID: <abc123@example.org>\n\
         Content-Type: text/plain; charset=utf-8\n\
         \n\
         Hello there, this is plain text.\n",
    );

    assert_eq!(parsed.subject.as_deref(), Some("Hello"));
    assert_eq!(parsed.from.len(), 1);
    assert_eq!(parsed.from[0].email.as_str(), "ada@example.org");
    assert_eq!(parsed.from[0].display_name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(parsed.to.len(), 1);
    assert_eq!(parsed.to[0].email.as_str(), "charles@example.com");
    assert_eq!(parsed.message_id.as_deref(), Some("abc123@example.org"));
    assert!(parsed.date.is_some());
    assert_eq!(parsed.date.unwrap().unix_seconds(), 1_751_362_200);
    assert_eq!(
        parsed.text_body.as_deref(),
        Some("Hello there, this is plain text.\r\n")
    );
    assert!(parsed.html_body.is_none());
    assert_eq!(parsed.preview, "Hello there, this is plain text.");
    assert!(parsed.attachments.is_empty());
    assert!(parsed.parse_warnings.is_empty());
}

#[test]
fn html_only_message_has_no_text_body_and_stripped_preview() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: HTML only\n\
         Content-Type: text/html; charset=utf-8\n\
         \n\
         <html><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>\n",
    );

    assert!(parsed.text_body.is_none());
    let html = parsed.html_body.expect("html body present");
    assert!(html.contains("<h1>Title</h1>"));
    assert!(parsed.preview.contains("Title"));
    assert!(parsed.preview.contains("Hello world"));
    assert!(!parsed.preview.contains('<'));
}

#[test]
fn multipart_alternative_extracts_both_bodies() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Alt\n\
         Content-Type: multipart/alternative; boundary=alt\n\
         \n\
         --alt\n\
         Content-Type: text/plain; charset=utf-8\n\
         \n\
         Plain version.\n\
         --alt\n\
         Content-Type: text/html; charset=utf-8\n\
         \n\
         <p>HTML version.</p>\n\
         --alt--\n",
    );

    // The CRLF that precedes a boundary belongs to the delimiter, so the
    // decoded body does not carry a trailing newline.
    assert_eq!(parsed.text_body.as_deref(), Some("Plain version."));
    assert!(
        parsed
            .html_body
            .as_deref()
            .unwrap()
            .contains("HTML version.")
    );
    assert_eq!(parsed.preview, "Plain version.");
    assert!(parsed.attachments.is_empty());
}

#[test]
fn multipart_mixed_attachment_round_trips_filename_and_bytes() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Mixed\n\
         Content-Type: multipart/mixed; boundary=mix\n\
         \n\
         --mix\n\
         Content-Type: text/plain; charset=utf-8\n\
         \n\
         See attachment.\n\
         --mix\n\
         Content-Type: text/plain; name=\"hello.txt\"\n\
         Content-Disposition: attachment; filename=\"hello.txt\"\n\
         Content-Transfer-Encoding: base64\n\
         \n\
         SGVsbG8sIGF0dGFjaG1lbnQh\n\
         --mix--\n",
    );

    assert_eq!(parsed.text_body.as_deref(), Some("See attachment."));
    assert_eq!(parsed.attachments.len(), 1);
    let attachment = &parsed.attachments[0];
    assert_eq!(attachment.filename.as_deref(), Some("hello.txt"));
    assert_eq!(attachment.mime_type, "text/plain");
    assert_eq!(attachment.bytes, b"Hello, attachment!");
    assert_eq!(attachment.size, 18);
    assert!(!attachment.inline);
    assert!(attachment.content_id.is_none());
}

#[test]
fn multipart_related_inline_cid_image_is_marked_inline() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Related\n\
         Content-Type: multipart/related; boundary=rel\n\
         \n\
         --rel\n\
         Content-Type: text/html; charset=utf-8\n\
         \n\
         <p>Image: <img src=\"cid:img1\"></p>\n\
         --rel\n\
         Content-Type: image/png\n\
         Content-ID: <img1>\n\
         Content-Disposition: inline\n\
         Content-Transfer-Encoding: base64\n\
         \n\
         iVBORw0KGgo=\n\
         --rel--\n",
    );

    assert!(parsed.html_body.as_deref().unwrap().contains("cid:img1"));
    assert_eq!(parsed.attachments.len(), 1);
    let image = &parsed.attachments[0];
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.content_id.as_deref(), Some("img1"));
    assert!(image.inline);
    assert!(image.filename.is_none());
    assert_eq!(image.size, 8);
    assert_eq!(image.bytes, [137, 80, 78, 71, 13, 10, 26, 10]);
}

#[test]
fn iso_8859_1_base64_subject_is_decoded() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: =?iso-8859-1?B?R3L832U=?=\n\
         Content-Type: text/plain; charset=utf-8\n\
         \n\
         body\n",
    );

    assert_eq!(parsed.subject.as_deref(), Some("Grüße"));
}

#[test]
fn quoted_printable_body_is_decoded() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: QP\n\
         Content-Type: text/plain; charset=utf-8\n\
         Content-Transfer-Encoding: quoted-printable\n\
         \n\
         Caf=C3=A9 and =E2=98=95\n",
    );

    assert_eq!(parsed.text_body.as_deref(), Some("Café and ☕\r\n"));
    assert_eq!(parsed.preview, "Café and ☕");
}

#[test]
fn broken_boundary_is_handled_gracefully() {
    let result = parse_message(&msg("From: a@example.org\n\
         Subject: Broken\n\
         Content-Type: multipart/mixed; boundary=bnd\n\
         \n\
         --bnd\n\
         Content-Type: text/plain\n\
         \n\
         Only an opening boundary, no closing marker.\n"));

    let parsed = result.expect("tolerant parse should not fail");
    assert_eq!(parsed.subject.as_deref(), Some("Broken"));
    // The content is still recoverable: an unclosed part is preserved either
    // as the text body or, when structure is ambiguous, as an attachment.
    let mut recovered = parsed.text_body.clone().unwrap_or_default();
    for attachment in &parsed.attachments {
        recovered.push_str(&String::from_utf8_lossy(&attachment.bytes));
    }
    assert!(recovered.contains("Only an opening boundary"));
}

#[test]
fn empty_body_yields_empty_preview() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Empty\n\
         \n",
    );

    assert_eq!(parsed.subject.as_deref(), Some("Empty"));
    assert_eq!(parsed.preview, "");
    assert!(parsed.attachments.is_empty());
}

#[test]
fn huge_header_line_is_accepted() {
    let subject = "X".repeat(8000);
    let raw = format!(
        "From: a@example.org\n\
         Subject: {subject}\n\
         Content-Type: text/plain\n\
         \n\
         body\n"
    );
    let parsed = parse(&raw);
    assert_eq!(parsed.subject.as_deref(), Some(subject.as_str()));
}

#[test]
fn message_with_no_from_has_empty_sender_list() {
    let parsed = parse(
        "To: b@example.org\n\
         Subject: No from\n\
         Content-Type: text/plain\n\
         \n\
         hi\n",
    );

    assert!(parsed.from.is_empty());
    assert_eq!(parsed.to.len(), 1);
}

#[test]
fn attachment_without_filename_keeps_none() {
    let parsed = parse(
        "From: a@example.org\n\
         Content-Type: multipart/mixed; boundary=nf\n\
         \n\
         --nf\n\
         Content-Type: text/plain\n\
         \n\
         body\n\
         --nf\n\
         Content-Type: application/octet-stream\n\
         Content-Disposition: attachment\n\
         Content-Transfer-Encoding: base64\n\
         \n\
         AAECAwQ=\n\
         --nf--\n",
    );

    assert_eq!(parsed.attachments.len(), 1);
    let attachment = &parsed.attachments[0];
    assert!(attachment.filename.is_none());
    assert_eq!(attachment.mime_type, "application/octet-stream");
    assert_eq!(attachment.size, 5);
    assert!(!attachment.inline);
    assert_eq!(attachment.bytes, [0, 1, 2, 3, 4]);
}

#[test]
fn duplicate_headers_do_not_break_parsing() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: First\n\
         Subject: Second\n\
         To: b@example.org\n\
         To: c@example.org\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert!(parsed.subject.is_some());
    assert!(!parsed.to.is_empty());
}

#[test]
fn missing_date_yields_none() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: No date\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );
    assert!(parsed.date.is_none());
}

#[test]
fn invalid_date_yields_none() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Bad date\n\
         Date: not a real date\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );
    assert!(parsed.date.is_none());
}

#[test]
fn cc_and_reply_to_are_parsed() {
    let parsed = parse(
        "From: a@example.org\n\
         To: b@example.org\n\
         Cc: c@example.org, d@example.org\n\
         Reply-To: reply@example.org\n\
         Subject: CC\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(parsed.cc.len(), 2);
    assert_eq!(parsed.cc[0].email.as_str(), "c@example.org");
    assert_eq!(parsed.cc[1].email.as_str(), "d@example.org");
    assert_eq!(
        parsed
            .reply_to
            .as_ref()
            .map(|address| address.email.as_str()),
        Some("reply@example.org")
    );
}

#[test]
fn malformed_addresses_are_dropped_and_redacted() {
    let parsed = parse(
        "From: a@example.org\n\
         To: Bad <user@nodot>, Good <good@example.org>\n\
         Subject: Warnings\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(parsed.to.len(), 1);
    assert_eq!(parsed.to[0].email.as_str(), "good@example.org");
    assert!(!parsed.parse_warnings.is_empty());
    let joined = parsed.parse_warnings.join(" ");
    assert!(joined.contains("to"));
    // The malformed value must never appear verbatim.
    assert!(!joined.contains("user@nodot"));
}

#[test]
fn nested_multipart_extracts_body_and_attachment() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: Nested\n\
         Content-Type: multipart/mixed; boundary=outer\n\
         \n\
         --outer\n\
         Content-Type: multipart/alternative; boundary=inner\n\
         \n\
         --inner\n\
         Content-Type: text/plain; charset=utf-8\n\
         \n\
         Nested plain.\n\
         --inner\n\
         Content-Type: text/html; charset=utf-8\n\
         \n\
         <p>Nested html.</p>\n\
         --inner--\n\
         --outer\n\
         Content-Type: text/plain; name=\"note.txt\"\n\
         Content-Disposition: attachment; filename=\"note.txt\"\n\
         \n\
         attached note\n\
         --outer--\n",
    );

    assert_eq!(parsed.text_body.as_deref(), Some("Nested plain."));
    assert!(
        parsed
            .html_body
            .as_deref()
            .unwrap()
            .contains("Nested html.")
    );
    assert_eq!(parsed.attachments.len(), 1);
    assert_eq!(parsed.attachments[0].filename.as_deref(), Some("note.txt"));
    assert_eq!(parsed.attachments[0].bytes, b"attached note");
}

#[test]
fn message_without_threading_headers_has_empty_chains() {
    let parsed = parse(
        "From: a@example.org\n\
         Subject: No threading\n\
         Message-ID: <root@example.org>\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(parsed.message_id.as_deref(), Some("root@example.org"));
    assert!(parsed.in_reply_to.is_empty());
    assert!(parsed.references.is_empty());
    assert!(parsed.threading_headers().parent_id().is_none());
}

#[test]
fn reply_exposes_in_reply_to_and_ordered_references() {
    let parsed = parse(
        "From: b@example.org\n\
         Subject: Re: Hello\n\
         Message-ID: <reply-2@example.org>\n\
         In-Reply-To: <root-0@example.org>\n\
         References: <root-0@example.org> <mid-1@example.org>\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(parsed.message_id.as_deref(), Some("reply-2@example.org"));
    assert_eq!(parsed.in_reply_to, vec!["root-0@example.org".to_owned()]);
    assert_eq!(
        parsed.references,
        vec![
            "root-0@example.org".to_owned(),
            "mid-1@example.org".to_owned()
        ]
    );
    // The nearest ancestor is the last reference.
    assert_eq!(
        parsed.threading_headers().parent_id(),
        Some("mid-1@example.org")
    );
}

#[test]
fn folded_references_header_is_unfolded_into_ordered_ids() {
    // A long References header wrapped across several continuation lines,
    // as real mailers emit for deep threads.
    let parsed = parse(
        "From: c@example.org\n\
         Subject: Re: deep thread\n\
         Message-ID: <r4@example.org>\n\
         References: <r0@example.org>\n\
         \t<r1@example.org>\n\
         \t<r2@example.org> <r3@example.org>\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(
        parsed.references,
        vec![
            "r0@example.org".to_owned(),
            "r1@example.org".to_owned(),
            "r2@example.org".to_owned(),
            "r3@example.org".to_owned(),
        ]
    );
}

#[test]
fn multiple_in_reply_to_ids_are_all_collected() {
    let parsed = parse(
        "From: d@example.org\n\
         Subject: merge\n\
         Message-ID: <merge@example.org>\n\
         In-Reply-To: <a@example.org> <b@example.org>\n\
         Content-Type: text/plain\n\
         \n\
         body\n",
    );

    assert_eq!(
        parsed.in_reply_to,
        vec!["a@example.org".to_owned(), "b@example.org".to_owned()]
    );
    assert!(parsed.references.is_empty());
    // With no References, the parent falls back to the first In-Reply-To id.
    assert_eq!(
        parsed.threading_headers().parent_id(),
        Some("a@example.org")
    );
}

#[test]
fn truly_unparseable_input_maps_to_parse_error() {
    // An empty byte stream has no message structure at all.
    let error = parse_message(&[]).expect_err("empty input should fail");
    assert!(matches!(error, axomail_core::MailError::Parse { .. }));
}

#[test]
fn parsed_message_round_trips_through_serde() {
    let parsed = parse(
        "From: Ada <ada@example.org>\n\
         To: b@example.org\n\
         Subject: Serde\n\
         Content-Type: multipart/mixed; boundary=s\n\
         \n\
         --s\n\
         Content-Type: text/plain\n\
         \n\
         hello\n\
         --s\n\
         Content-Type: application/octet-stream\n\
         Content-Disposition: attachment; filename=\"blob.bin\"\n\
         Content-Transfer-Encoding: base64\n\
         \n\
         AAECAwQ=\n\
         --s--\n",
    );

    let json = serde_json::to_string(&parsed).expect("serialize");
    let restored: ParsedMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, restored);
}
