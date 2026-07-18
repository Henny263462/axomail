//! Email address value types.
//!
//! Validation here is deliberately light and syntactic: enough to catch
//! obvious configuration mistakes early. Full RFC 5322 handling of
//! addresses inside message headers is the job of `axomail-parser`.

use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// Maximum total length accepted for an address (RFC 3696 erratum 1690).
const MAX_ADDRESS_LENGTH: usize = 254;

/// A syntactically validated email address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmailAddress(String);

impl EmailAddress {
    /// Parses and lightly validates an address.
    ///
    /// # Errors
    ///
    /// Returns [`MailError::Configuration`] when the input is empty, has
    /// no or more than one `@`, an empty local part or domain, exceeds
    /// 254 characters, or contains whitespace/control characters.
    pub fn parse(input: &str) -> Result<Self, MailError> {
        let invalid = |message: &str| MailError::Configuration {
            message: format!("invalid email address: {message}"),
        };

        if input.is_empty() {
            return Err(invalid("empty input"));
        }
        if input.len() > MAX_ADDRESS_LENGTH {
            return Err(invalid("longer than 254 characters"));
        }
        if input.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(invalid("contains whitespace or control characters"));
        }
        let mut parts = input.split('@');
        let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(invalid("must contain exactly one '@'"));
        };
        if local.is_empty() {
            return Err(invalid("empty local part"));
        }
        if domain.is_empty() || !domain.contains('.') {
            return Err(invalid("domain must contain a dot"));
        }
        Ok(Self(input.to_owned()))
    }

    /// Returns the full address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the part before the `@`.
    #[must_use]
    pub fn local_part(&self) -> &str {
        self.0.split('@').next().unwrap_or_default()
    }

    /// Returns the part after the `@`.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split('@').nth(1).unwrap_or_default()
    }
}

impl TryFrom<String> for EmailAddress {
    type Error = MailError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<EmailAddress> for String {
    fn from(value: EmailAddress) -> Self {
        value.0
    }
}

impl std::fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A display name plus email address, as used in message headers and
/// the composer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    /// Optional human-readable name (`"Ada Lovelace"`).
    pub display_name: Option<String>,
    /// The address itself.
    pub email: EmailAddress,
}

impl Address {
    /// Creates an address without a display name.
    #[must_use]
    pub fn from_email(email: EmailAddress) -> Self {
        Self {
            display_name: None,
            email,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn accepts_common_addresses() {
        for input in [
            "a@example.org",
            "first.last@mail.example.co.uk",
            "user+tag@example.org",
        ] {
            let parsed = EmailAddress::parse(input).unwrap();
            assert_eq!(parsed.as_str(), input);
        }
    }

    #[test]
    fn splits_local_part_and_domain() {
        let parsed = EmailAddress::parse("user+tag@example.org").unwrap();
        assert_eq!(parsed.local_part(), "user+tag");
        assert_eq!(parsed.domain(), "example.org");
    }

    #[test]
    fn rejects_malformed_addresses() {
        for input in [
            "",
            "no-at-sign",
            "two@@example.org",
            "a@b@c.org",
            "@example.org",
            "user@",
            "user@nodot",
            "with space@example.org",
            "tab\t@example.org",
        ] {
            assert!(
                EmailAddress::parse(input).is_err(),
                "should reject {input:?}"
            );
        }
    }

    #[test]
    fn rejects_overlong_addresses() {
        let input = format!("{}@example.org", "x".repeat(250));
        assert!(EmailAddress::parse(&input).is_err());
    }

    #[test]
    fn serde_validates_on_deserialize() {
        let ok: Result<EmailAddress, _> = serde_json::from_str("\"a@example.org\"");
        assert!(ok.is_ok());
        let bad: Result<EmailAddress, _> = serde_json::from_str("\"not-an-address\"");
        assert!(bad.is_err());
    }
}
