//! PKCE (RFC 7636) code verifier/challenge and the CSRF `state` parameter.
//!
//! The verifier is 32 bytes of CSPRNG output, base64url-encoded (43 chars, no
//! padding). The `S256` challenge is `base64url(sha256(verifier))`. The
//! `state` parameter is 16 independent random bytes, base64url-encoded, and is
//! compared against the value echoed back on the loopback redirect to defeat
//! authorization-response CSRF.
//!
//! Randomness and SHA-256 both come from `ring`, which is already the process
//! rustls crypto provider, so no additional dependency is introduced.

use axomail_core::MailError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};

/// A PKCE verifier/challenge pair for one authorization request.
#[derive(Clone)]
pub struct Pkce {
    /// The secret code verifier, sent only to the token endpoint.
    pub verifier: String,
    /// The `S256` code challenge, sent in the authorization URL.
    pub challenge: String,
}

impl Pkce {
    /// Generates a fresh verifier and its `S256` challenge.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] only if the system CSPRNG fails,
    /// which is not expected on a functioning platform.
    pub fn generate() -> Result<Self, MailError> {
        let verifier_bytes = random_bytes::<32>()?;
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref());
        Ok(Self {
            verifier,
            challenge,
        })
    }
}

/// Generates a random, URL-safe CSRF `state` value.
///
/// # Errors
///
/// Returns [`MailError::Configuration`] if the system CSPRNG fails.
pub fn random_state() -> Result<String, MailError> {
    Ok(URL_SAFE_NO_PAD.encode(random_bytes::<16>()?))
}

/// Fills an `N`-byte array from the system CSPRNG.
fn random_bytes<const N: usize>() -> Result<[u8; N], MailError> {
    let mut bytes = [0u8; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| MailError::Configuration {
            message: "could not gather secure randomness for the sign-in request".to_owned(),
        })?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn challenge_is_s256_of_verifier() {
        let pkce = Pkce::generate().unwrap();
        // Recompute the challenge independently and compare.
        let expected = URL_SAFE_NO_PAD.encode(digest(&SHA256, pkce.verifier.as_bytes()).as_ref());
        assert_eq!(pkce.challenge, expected);
        // base64url(sha256) is always 43 chars unpadded.
        assert_eq!(pkce.challenge.len(), 43);
    }

    #[test]
    fn verifier_is_unpadded_url_safe_and_long_enough() {
        let pkce = Pkce::generate().unwrap();
        assert_eq!(pkce.verifier.len(), 43);
        assert!(!pkce.verifier.contains('='));
        assert!(!pkce.verifier.contains('+'));
        assert!(!pkce.verifier.contains('/'));
    }

    #[test]
    fn successive_values_differ() {
        let a = Pkce::generate().unwrap();
        let b = Pkce::generate().unwrap();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(random_state().unwrap(), random_state().unwrap());
    }
}
