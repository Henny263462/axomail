//! Remote-content policy for HTML sanitization (ADR-0007).
//!
//! By default the sanitizer blocks every remote resource (tracking-pixel
//! defense). This policy is the single, explicit knob that relaxes exactly
//! one thing: whether remote `http(s)` `img@src` references are kept so the
//! WebView loads them. It never widens any other rule — `data:` URLs (rejected
//! at the scheme gate), `srcset`, CSS, scripts, the link scheme allowlist, and
//! every other strip stay exactly as they are with blocking on. See
//! [`crate::html`] for how the attribute filter consumes it.

/// Whether remote `http(s)` images may be kept during sanitization.
///
/// This is the *only* dimension of the sanitizer that this type changes.
/// Constructing it with `allow_remote_images = true` corresponds to the user
/// choosing "show remote content" for a message or sender; the sanitizer then
/// keeps `http(s)` `img@src` targets and does not count them as blocked, while
/// continuing to reject `data:` URLs at the scheme gate and to strip and count
/// `srcset` — those remain rejected in every mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteContentPolicy {
    /// Keep remote `http(s)` image sources instead of blocking and counting
    /// them. All other resource and scheme rules are unaffected.
    pub allow_remote_images: bool,
}

impl RemoteContentPolicy {
    /// The default, most conservative policy: block every remote resource.
    #[must_use]
    pub const fn block_all() -> Self {
        Self {
            allow_remote_images: false,
        }
    }

    /// A policy that keeps remote `http(s)` images (allow-once / allowed
    /// sender). Nothing else is loosened.
    #[must_use]
    pub const fn allow_images() -> Self {
        Self {
            allow_remote_images: true,
        }
    }
}

impl Default for RemoteContentPolicy {
    /// Blocking every remote resource is the safe default.
    fn default() -> Self {
        Self::block_all()
    }
}
