//! F7.2 — Notifier service.
//!
//! Desktop notifications are platform-specific and bring in heavy
//! deps (notify-rust pulls D-Bus on Linux, NSUserNotification on mac,
//! toast APIs on Windows). We want the engine/TUI to emit
//! notifications *through a trait* so:
//!
//! 1. Unit tests can plug in a capturing impl.
//! 2. Headless runs (`vac run`) can plug in `TracingNotifier` that
//!    just logs.
//! 3. A future `notify-rust`-backed impl lands as a separate crate
//!    without touching consumers.
//!
//! This module intentionally does not depend on notify-rust.

use std::sync::Mutex;

/// Severity/category. Maps to the platform's notification urgency
/// when the real desktop backend is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotifyLevel {
    /// Build/task finished successfully; low priority.
    Info,
    /// Operator action required (e.g. permission prompt timed out).
    Warn,
    /// Build/task failed; highest visible priority.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub level: NotifyLevel,
    pub title: String,
    pub body: String,
}

pub trait Notifier: Send + Sync {
    fn notify(&self, note: Notification);
}

/// No-op. Useful as a default in tests or headless runs that don't
/// want any notification surface at all.
#[derive(Debug, Default)]
pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn notify(&self, _note: Notification) {}
}

/// Emits every notification through `tracing`. Good fit for CLI
/// headless runs where the terminal IS the notification channel.
#[derive(Debug, Default)]
pub struct TracingNotifier;

impl Notifier for TracingNotifier {
    fn notify(&self, note: Notification) {
        match note.level {
            NotifyLevel::Info => tracing::info!(
                target: "vac_tui_runtime::notifier",
                title = %note.title,
                "{}", note.body
            ),
            NotifyLevel::Warn => tracing::warn!(
                target: "vac_tui_runtime::notifier",
                title = %note.title,
                "{}", note.body
            ),
            NotifyLevel::Error => tracing::error!(
                target: "vac_tui_runtime::notifier",
                title = %note.title,
                "{}", note.body
            ),
        }
    }
}

/// In-memory capture for tests + snapshot assertions.
#[derive(Debug, Default)]
pub struct CapturingNotifier {
    inner: Mutex<Vec<Notification>>,
}

impl CapturingNotifier {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<Notification> {
        self.inner.lock().ok().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut v) = self.inner.lock() {
            v.clear();
        }
    }
}

impl Notifier for CapturingNotifier {
    fn notify(&self, note: Notification) {
        if let Ok(mut v) = self.inner.lock() {
            v.push(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_notifier_swallows_silently() {
        NullNotifier.notify(Notification {
            level: NotifyLevel::Info,
            title: "build ok".into(),
            body: "cargo build succeeded".into(),
        });
        // No assertion possible other than "did not panic".
    }

    #[test]
    fn capturing_notifier_records_in_order() {
        let n = CapturingNotifier::new();
        n.notify(Notification {
            level: NotifyLevel::Info,
            title: "a".into(),
            body: "1".into(),
        });
        n.notify(Notification {
            level: NotifyLevel::Error,
            title: "b".into(),
            body: "2".into(),
        });
        let snap = n.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].title, "a");
        assert_eq!(snap[1].level, NotifyLevel::Error);
    }

    #[test]
    fn capturing_notifier_clear_empties() {
        let n = CapturingNotifier::new();
        n.notify(Notification {
            level: NotifyLevel::Warn,
            title: "x".into(),
            body: "y".into(),
        });
        n.clear();
        assert!(n.snapshot().is_empty());
    }
}
