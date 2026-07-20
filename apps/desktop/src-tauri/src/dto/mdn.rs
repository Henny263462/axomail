//! DTO for the outcome of sending a read receipt (MDN).

use serde::Serialize;

/// The result of a `send_mdn` call.
///
/// `status` is a stable slug the frontend maps to a message: `"sent"` (a
/// receipt was delivered), `"alreadySent"` (a receipt had already been sent —
/// no-op), `"suppressed"` (the honour policy decided not to send, e.g. a
/// mailing-list message), or `"failed"` (delivery failed; `error` carries the
/// redacted reason). `sent` is `true` only for `"sent"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdnOutcomeDto {
    /// Whether a receipt was actually delivered.
    pub sent: bool,
    /// Stable outcome slug (see the type docs).
    pub status: &'static str,
    /// Redacted failure reason when `status` is `"failed"`.
    pub error: Option<String>,
}

impl MdnOutcomeDto {
    /// A receipt was delivered.
    #[must_use]
    pub fn sent() -> Self {
        Self {
            sent: true,
            status: "sent",
            error: None,
        }
    }

    /// A receipt had already been sent; nothing was sent this time.
    #[must_use]
    pub fn already_sent() -> Self {
        Self {
            sent: false,
            status: "alreadySent",
            error: None,
        }
    }

    /// The honour policy decided not to send a receipt for this message.
    #[must_use]
    pub fn suppressed() -> Self {
        Self {
            sent: false,
            status: "suppressed",
            error: None,
        }
    }

    /// Delivery failed; `error` is a redacted reason.
    #[must_use]
    pub fn failed(error: String) -> Self {
        Self {
            sent: false,
            status: "failed",
            error: Some(error),
        }
    }
}
