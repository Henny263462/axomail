//! RFC 3156 PGP/MIME framing: wrap an outgoing message as `multipart/signed`
//! and/or `multipart/encrypted`, and recognise and decompose an incoming one.
//!
//! The delicate invariant is byte-exactness on the signed part: a
//! `multipart/signed` signature covers the *transport* bytes of the signed MIME
//! entity (its own headers, the blank line, and its body), CRLF-canonical, up to
//! but not including the CRLF that introduces the boundary delimiter (RFC 3156
//! §5). [`apply_pgp_mime`] signs exactly the bytes it then writes, and
//! [`parse_pgp_mime`] extracts exactly those bytes back, so a round-trip — and
//! interop with other RFC 3156 implementations — verifies.

use super::mime_util::{
    boundary_of, canonicalize_crlf, extract_armor, header_name, header_value, is_content_header,
    make_boundary, multipart_parts, normalize_armor, parse_logical_headers, split_headers_body,
};
use super::{encrypt::encrypt_message, sign::sign_detached};
use axomail_core::MailError;

/// What an incoming message's OpenPGP structure turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgpMimeMessage {
    /// RFC 3156 `multipart/signed`. `signed_entity` is the exact canonical byte
    /// range the signature covers; `signature` is the armored detached
    /// signature from the `application/pgp-signature` part.
    SignedMime {
        /// Exact transport bytes covered by the signature.
        signed_entity: Vec<u8>,
        /// Armored detached signature.
        signature: String,
    },
    /// RFC 3156 `multipart/encrypted`. `ciphertext` is the armored OpenPGP
    /// message from the `application/octet-stream` part.
    EncryptedMime {
        /// Armored OpenPGP encrypted message.
        ciphertext: String,
    },
    /// A bare armored OpenPGP message in the body (legacy inline PGP).
    InlineEncrypted {
        /// Armored OpenPGP encrypted message.
        ciphertext: String,
    },
}

/// Content-Type header value written for the signed and encrypted control
/// parts, and the boundary tags.
const SIG_PROTOCOL: &str = "application/pgp-signature";
const ENC_PROTOCOL: &str = "application/pgp-encrypted";

/// Wraps `message` (a complete RFC 5322 byte stream) as PGP/MIME per the
/// requested protection, returning the transformed message.
///
/// The transform preserves the envelope headers (From/To/Subject/Date/
/// Message-ID/MIME-Version, …) and moves the `Content-*` headers plus the body
/// into the protected inner MIME entity. `signer` is `(secret_armored,
/// passphrase)` for signing; `encrypt_to` is the recipient public keys (empty =
/// no encryption). Signing and encryption compose as `encrypt(sign(content))`
/// (RFC 3156 §6.1). With neither requested, the message is returned unchanged.
///
/// # Errors
///
/// Propagates [`MailError::Crypto`] / [`MailError::Configuration`] from the
/// underlying sign/encrypt steps.
pub fn apply_pgp_mime(
    message: &[u8],
    signer: Option<(&str, Option<&str>)>,
    encrypt_to: &[String],
) -> Result<Vec<u8>, MailError> {
    if signer.is_none() && encrypt_to.is_empty() {
        return Ok(message.to_vec());
    }
    let canonical = canonicalize_crlf(message);
    let (header_block, body) = split_headers_body(&canonical);
    let headers = parse_logical_headers(&header_block);
    let (envelope, content): (Vec<&str>, Vec<&str>) = headers
        .iter()
        .map(String::as_str)
        .partition(|line| !is_content_header(line));

    let inner_entity = build_entity(&content, &body);

    // Sign first, if requested, producing a self-contained multipart/signed
    // MIME entity; then optionally encrypt that (or the plain inner entity).
    let (protected_ct, protected_body) = if let Some((secret, passphrase)) = signer {
        let signature = sign_detached(secret, passphrase, &inner_entity)?;
        signed_multipart(&inner_entity, &signature)
    } else {
        // Encryption-only: the protected payload is the inner entity itself.
        let ct = content_type_of(&content);
        (ct, inner_entity.clone())
    };

    let (top_ct, top_body) = if encrypt_to.is_empty() {
        (protected_ct, protected_body)
    } else {
        let payload = assemble_entity(&protected_ct, &protected_body);
        let ciphertext = encrypt_message(encrypt_to, &payload)?;
        encrypted_multipart(&ciphertext)
    };

    Ok(reassemble(&envelope, &top_ct, &top_body))
}

/// Recognises the OpenPGP structure of a received message, if any.
///
/// Returns `None` for a message that carries no OpenPGP payload. Only structure
/// is decoded here — no keys are consulted and nothing is verified or decrypted.
#[must_use]
pub fn parse_pgp_mime(raw: &[u8]) -> Option<PgpMimeMessage> {
    let (header_block, body) = split_headers_body(raw);
    let content_type = header_value(&header_block, "content-type").unwrap_or_default();
    let lower = content_type.to_ascii_lowercase();

    if lower.contains("multipart/signed")
        && lower.contains(SIG_PROTOCOL)
        && let Some(boundary) = boundary_of(&content_type)
    {
        let parts = multipart_parts(&body, &boundary);
        if parts.len() >= 2 {
            let signed_entity = parts[0].to_vec();
            let (_h, sig_body) = split_headers_body(parts[1]);
            if let Some(signature) = extract_armor(&sig_body, "SIGNATURE") {
                return Some(PgpMimeMessage::SignedMime {
                    signed_entity,
                    signature,
                });
            }
        }
    }

    if lower.contains("multipart/encrypted")
        && lower.contains(ENC_PROTOCOL)
        && let Some(boundary) = boundary_of(&content_type)
    {
        let parts = multipart_parts(&body, &boundary);
        // The last part is the application/octet-stream ciphertext.
        for part in parts.iter().rev() {
            let (_h, part_body) = split_headers_body(part);
            if let Some(ciphertext) = extract_armor(&part_body, "MESSAGE") {
                return Some(PgpMimeMessage::EncryptedMime { ciphertext });
            }
        }
    }

    // Inline PGP: a bare armored message in the body.
    if let Some(ciphertext) = extract_armor(&body, "MESSAGE") {
        return Some(PgpMimeMessage::InlineEncrypted { ciphertext });
    }

    None
}

/// Builds the `multipart/signed` control structure over an inner entity.
fn signed_multipart(inner_entity: &[u8], signature: &str) -> (String, Vec<u8>) {
    let boundary = make_boundary("sig");
    let ct = format!(
        "Content-Type: multipart/signed; micalg=\"pgp-sha256\"; \
         protocol=\"{SIG_PROTOCOL}\"; boundary=\"{boundary}\""
    );
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(inner_entity);
    body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Type: {SIG_PROTOCOL}; name=\"signature.asc\"\r\n\
             Content-Description: OpenPGP digital signature\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(normalize_armor(signature).as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (ct, body)
}

/// Builds the `multipart/encrypted` control structure around a ciphertext.
fn encrypted_multipart(ciphertext: &str) -> (String, Vec<u8>) {
    let boundary = make_boundary("enc");
    let ct = format!(
        "Content-Type: multipart/encrypted; protocol=\"{ENC_PROTOCOL}\"; boundary=\"{boundary}\""
    );
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Type: {ENC_PROTOCOL}\r\n\
             Content-Description: PGP/MIME version identification\r\n\r\nVersion: 1\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
          Content-Description: OpenPGP encrypted message\r\n\
          Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n",
    );
    body.extend_from_slice(normalize_armor(ciphertext).as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (ct, body)
}

/// Joins content headers with the body into a MIME entity (headers, blank line,
/// body). Falls back to `text/plain` if the source carried no content headers.
fn build_entity(content_headers: &[&str], body: &[u8]) -> Vec<u8> {
    let ct = content_type_of(content_headers);
    if content_headers.is_empty() {
        return assemble_entity(&ct, body);
    }
    let mut out = Vec::new();
    for (index, header) in content_headers.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(header.as_bytes());
    }
    out.extend_from_slice(b"\r\n\r\n");
    out.extend_from_slice(body);
    out
}

/// Assembles a MIME entity from a single content-type header line and a body.
fn assemble_entity(content_type_header: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(content_type_header.as_bytes());
    out.extend_from_slice(b"\r\n\r\n");
    out.extend_from_slice(body);
    out
}

/// The first content-type header line among `content_headers`, or a plain-text
/// default.
fn content_type_of(content_headers: &[&str]) -> String {
    content_headers
        .iter()
        .find(|line| header_name(line).eq_ignore_ascii_case("content-type"))
        .map_or_else(
            || "Content-Type: text/plain; charset=utf-8".to_owned(),
            |line| (*line).to_owned(),
        )
}

/// Reassembles the final message: envelope headers, then the new top-level
/// content-type, a blank line, and the body.
fn reassemble(envelope: &[&str], top_ct: &str, top_body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for header in envelope {
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(top_ct.as_bytes());
    out.extend_from_slice(b"\r\n\r\n");
    out.extend_from_slice(top_body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_pgp_message_is_not_recognised() {
        let raw = b"From: a@b.c\r\nContent-Type: text/plain\r\n\r\nhello\r\n";
        assert_eq!(parse_pgp_mime(raw), None);
    }
}
