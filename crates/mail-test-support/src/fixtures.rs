//! Realistic RFC 5322 test messages shared by parser and adapter tests.
//!
//! Each fixture is a complete message with CRLF line endings, exercising a
//! specific parsing concern. They are deliberately small but structurally
//! faithful. [`ALL`] enumerates them as `(name, bytes)` pairs for corpus
//! iteration.

/// A plain `text/plain` message with a UTF-8 body.
pub const PLAIN: &str = concat!(
    "From: Alice Example <alice@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Lunch on Thursday\r\n",
    "Date: Tue, 14 Jul 2026 09:15:00 +0000\r\n",
    "Message-ID: <plain-0001@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "Hi Bob,\r\n",
    "\r\n",
    "Are we still on for lunch on Thursday?\r\n",
    "\r\n",
    "-- Alice\r\n",
);

/// A `multipart/alternative` message with plain and HTML parts.
pub const ALTERNATIVE: &str = concat!(
    "From: Alice Example <alice@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Release notes\r\n",
    "Date: Wed, 15 Jul 2026 12:00:00 +0000\r\n",
    "Message-ID: <alt-0002@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/alternative; boundary=\"alt-boundary-42\"\r\n",
    "\r\n",
    "--alt-boundary-42\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "Release 2.0 is out. See the site for details.\r\n",
    "\r\n",
    "--alt-boundary-42\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "<html><body><p>Release <b>2.0</b> is out.</p></body></html>\r\n",
    "\r\n",
    "--alt-boundary-42--\r\n",
);

/// A `multipart/mixed` message with a text part and a base64 attachment.
pub const MIXED_WITH_ATTACHMENT: &str = concat!(
    "From: Alice Example <alice@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Invoice attached\r\n",
    "Date: Thu, 16 Jul 2026 08:30:00 +0000\r\n",
    "Message-ID: <mixed-0003@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/mixed; boundary=\"mixed-boundary-7\"\r\n",
    "\r\n",
    "--mixed-boundary-7\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "Hi Bob,\r\n",
    "\r\n",
    "The invoice is attached as a text file.\r\n",
    "\r\n",
    "--mixed-boundary-7\r\n",
    "Content-Type: text/plain; name=\"invoice.txt\"\r\n",
    "Content-Transfer-Encoding: base64\r\n",
    "Content-Disposition: attachment; filename=\"invoice.txt\"\r\n",
    "\r\n",
    "SW52b2ljZSAjMTIzNApUb3RhbCBkdWU6IEVVUiA0Mi4wMAo=\r\n",
    "\r\n",
    "--mixed-boundary-7--\r\n",
);

/// A `multipart/related` message whose HTML references an inline image by
/// `cid:` URL, with the image carried in a second part.
pub const RELATED_WITH_CID: &str = concat!(
    "From: Alice Example <alice@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Newsletter with logo\r\n",
    "Date: Thu, 16 Jul 2026 15:45:00 +0000\r\n",
    "Message-ID: <related-0004@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/related; type=\"text/html\"; boundary=\"rel-boundary-9\"\r\n",
    "\r\n",
    "--rel-boundary-9\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 7bit\r\n",
    "\r\n",
    "<html><body><img src=\"cid:logo-1@example.org\" alt=\"logo\"></body></html>\r\n",
    "\r\n",
    "--rel-boundary-9\r\n",
    "Content-Type: image/png\r\n",
    "Content-Transfer-Encoding: base64\r\n",
    "Content-ID: <logo-1@example.org>\r\n",
    "Content-Disposition: inline; filename=\"logo.png\"\r\n",
    "\r\n",
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M9QDwADhgGAWjR9\r\n",
    "awAAAABJRU5ErkJggg==\r\n",
    "\r\n",
    "--rel-boundary-9--\r\n",
);

/// A `multipart/mixed` message whose closing delimiter does not match the
/// declared boundary, exercising tolerant parsing of malformed structure.
pub const BROKEN_BOUNDARY: &str = concat!(
    "From: Alice Example <alice@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Slightly broken multipart\r\n",
    "Date: Fri, 17 Jul 2026 10:05:00 +0000\r\n",
    "Message-ID: <broken-0005@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: multipart/mixed; boundary=\"correct-boundary\"\r\n",
    "\r\n",
    "--correct-boundary\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "\r\n",
    "This part opens correctly but the message never closes its boundary.\r\n",
    "\r\n",
    "--wrong-boundary--\r\n",
);

/// A message whose `Subject` carries raw 8-bit UTF-8 and whose body is
/// declared `8bit`, exercising non-ASCII header and body handling.
pub const EIGHT_BIT_SUBJECT: &str = concat!(
    "From: Renée Dupont <renee@example.org>\r\n",
    "To: Bob Example <bob@example.com>\r\n",
    "Subject: Grüße aus München – Preis: 10€\r\n",
    "Date: Fri, 17 Jul 2026 18:20:00 +0000\r\n",
    "Message-ID: <8bit-0006@example.org>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "Content-Transfer-Encoding: 8bit\r\n",
    "\r\n",
    "Hallo Bob,\r\n",
    "\r\n",
    "schöne Grüße und bis bald – Renée\r\n",
);

/// Every fixture as a `(name, bytes)` pair, for corpus-style iteration.
pub const ALL: &[(&str, &str)] = &[
    ("plain", PLAIN),
    ("alternative", ALTERNATIVE),
    ("mixed_with_attachment", MIXED_WITH_ATTACHMENT),
    ("related_with_cid", RELATED_WITH_CID),
    ("broken_boundary", BROKEN_BOUNDARY),
    ("eight_bit_subject", EIGHT_BIT_SUBJECT),
];
