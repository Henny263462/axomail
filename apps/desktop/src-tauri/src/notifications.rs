//! Desktop new-mail notifications (phase 4).
//!
//! When a sync job finishes having ingested new messages and the user has
//! notifications enabled, the app shows exactly one OS notification carrying
//! only an aggregate count — never a subject, sender, or any other message
//! content (privacy by default). The count is per finished job (already summed
//! over that job's passes).
//!
//! The OS call is isolated behind [`NewMailNotifier`] so the decision logic in
//! [`maybe_notify_new_messages`] is unit-testable; the real
//! [`TauriNotifier`] cannot be exercised in CI.

/// Shows an aggregate "new messages" notification. Implementors must never
/// include message content — only the count.
pub trait NewMailNotifier: Send + Sync {
    /// Displays a single notification announcing `count` new messages.
    fn notify_new_messages(&self, count: u64);
}

/// Shows a notification only when notifications are enabled and at least one
/// new message arrived. Returns whether a notification was shown.
///
/// This is the whole emission policy, kept free of any OS dependency so it can
/// be tested against a recording notifier.
pub fn maybe_notify_new_messages(
    notifier: &dyn NewMailNotifier,
    enabled: bool,
    count: u64,
) -> bool {
    if enabled && count > 0 {
        notifier.notify_new_messages(count);
        true
    } else {
        false
    }
}

/// Formats the count-only notification body, pluralizing for readability.
///
/// Deliberately content-free: the string contains nothing but the number.
#[must_use]
pub fn new_messages_body(count: u64) -> String {
    if count == 1 {
        "1 new message".to_owned()
    } else {
        format!("{count} new messages")
    }
}

/// Production [`NewMailNotifier`] backed by `tauri-plugin-notification`.
///
/// Cheap to clone (an `AppHandle` is an `Arc` internally), so the sync-event
/// forwarder can own one.
#[derive(Clone)]
pub struct TauriNotifier {
    app: tauri::AppHandle,
}

impl TauriNotifier {
    /// Wraps a Tauri app handle for showing notifications.
    #[must_use]
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl NewMailNotifier for TauriNotifier {
    fn notify_new_messages(&self, count: u64) {
        use tauri_plugin_notification::NotificationExt;
        // Title is a fixed app label; the body is the count only.
        let result = self
            .app
            .notification()
            .builder()
            .title("axomail")
            .body(new_messages_body(count))
            .show();
        if let Err(error) = result {
            tracing::warn!(error = %error, "failed to show new-mail notification");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::sync::Mutex;

    use super::{NewMailNotifier, maybe_notify_new_messages, new_messages_body};

    /// Records the counts it was asked to notify, for assertions.
    #[derive(Default)]
    struct RecordingNotifier {
        counts: Mutex<Vec<u64>>,
    }

    impl RecordingNotifier {
        fn counts(&self) -> Vec<u64> {
            self.counts.lock().expect("lock").clone()
        }
    }

    impl NewMailNotifier for RecordingNotifier {
        fn notify_new_messages(&self, count: u64) {
            self.counts.lock().expect("lock").push(count);
        }
    }

    #[test]
    fn notifies_when_enabled_and_new_mail_arrived() {
        let notifier = RecordingNotifier::default();
        assert!(maybe_notify_new_messages(&notifier, true, 3));
        assert_eq!(notifier.counts(), vec![3]);
    }

    #[test]
    fn silent_when_no_new_messages() {
        let notifier = RecordingNotifier::default();
        assert!(!maybe_notify_new_messages(&notifier, true, 0));
        assert!(notifier.counts().is_empty());
    }

    #[test]
    fn silent_when_disabled_even_with_new_mail() {
        let notifier = RecordingNotifier::default();
        assert!(!maybe_notify_new_messages(&notifier, false, 5));
        assert!(notifier.counts().is_empty());
    }

    #[test]
    fn body_carries_only_the_count() {
        assert_eq!(new_messages_body(1), "1 new message");
        assert_eq!(new_messages_body(4), "4 new messages");
    }
}
