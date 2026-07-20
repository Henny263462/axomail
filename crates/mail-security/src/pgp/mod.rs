//! OpenPGP support (Track 5.1, ADR-0015).
//!
//! A thin, security-focused layer over the pure-Rust [`pgp`](pgp) (rPGP) crate.
//! It provides the four operations a mail client needs — **sign**, **verify**,
//! **encrypt**, **decrypt** — plus key management, and wraps them in RFC 3156
//! PGP/MIME framing for the on-the-wire message structure.
//!
//! # Boundaries
//!
//! - **Secrets stay out of the database and logs** (ADR-0001). A secret key is
//!   handled only as an armored blob destined for the OS keychain via
//!   [`SecretStore`](crate::SecretStore); [`credential_ref_for_pgp`] builds its
//!   reference. Public keys and metadata are non-secret and may be persisted.
//! - **Errors are redacted** (ADR-0003): a crypto failure becomes a
//!   [`MailError::Crypto`] with a fixed, secret-free message; a malformed key or
//!   input the *user* supplied becomes a [`MailError::Configuration`]. Raw rPGP
//!   error detail, key bytes, and passphrases are never carried across the IPC
//!   boundary.
//! - **A bad signature is a result, not an error.** Verification returns a
//!   [`SignatureVerification`] status (`Valid` / `Invalid` / `UnknownKey`); only
//!   an internal fault surfaces as an error.
//!
//! All operations here are local CPU work with no network I/O, so no
//! `with_deadline` wrapping is needed; callers on an async runtime should still
//! offload the heavier ones (key generation, decrypt) to a blocking context.

mod enc_select;
mod encrypt;
mod keys;
mod mime;
mod mime_util;
mod secret;
mod sign;
mod sign_verify;

pub use enc_select::public_key_can_encrypt;
pub use encrypt::{DecryptedMessage, decrypt_message, encrypt_message};
pub use keys::{
    GeneratedKey, KeyMetadata, generate_keypair, import_public_key, import_secret_key,
    public_from_secret,
};
pub use mime::{PgpMimeMessage, apply_pgp_mime, parse_pgp_mime};
pub use secret::credential_ref_for_pgp;
pub use sign::{SignatureVerification, VerifiedSignature, sign_detached, verify_detached};

use axomail_core::MailError;

/// Builds a redacted [`MailError::Crypto`] for an internal crypto failure.
///
/// `step` is a fixed, caller-supplied description of the failing operation. The
/// underlying rPGP error is intentionally discarded so no key material or
/// low-level detail can leak (ADR-0003).
pub(crate) fn crypto_err(step: &str) -> MailError {
    MailError::Crypto {
        message: format!("could not {step}"),
    }
}

/// Builds a [`MailError::Configuration`] for user-supplied key material that
/// could not be parsed (a bad paste, a wrong block type, truncated armor).
pub(crate) fn invalid_key(kind: &str) -> MailError {
    MailError::Configuration {
        message: format!("the provided {kind} is not valid OpenPGP armored data"),
    }
}
