//! F7.3 — Prevent-sleep wake lock.
//!
//! Long agent turns (multi-minute builds, large test sweeps) should
//! keep the machine awake so the operator can come back to a finished
//! task. The actual OS calls differ per platform:
//!
//! - macOS: `caffeinate` subprocess or IOKit `IOPMAssertionCreate`.
//! - Linux: systemd-inhibit or `xdg-screensaver suspend`.
//! - Windows: `SetThreadExecutionState`.
//!
//! This module defines the trait + a no-op impl + an RAII guard. Real
//! platform backends wire in via downstream crates or feature flags;
//! the engine codepath stays the same.

/// Reason for the wake-lock. Shown to the OS so the user can inspect
/// which process is holding the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeReason {
    pub summary: String,
}

impl WakeReason {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
        }
    }
}

/// Acquire a wake-lock; drop the returned guard to release it. The
/// trait is infallible because all platform backends degrade to
/// "do nothing" if the OS refuses — starving an agent turn because
/// the wake-lock couldn't be taken is worse than letting the
/// machine sleep.
pub trait WakeLock: Send + Sync {
    fn acquire(&self, reason: WakeReason) -> Box<dyn WakeGuard>;
}

/// RAII token. Drop to release. Kept as a trait object so downstream
/// backends can attach platform-specific state.
pub trait WakeGuard: Send {
    /// Informational label — for tracing/diagnostics. Not machine-read.
    fn label(&self) -> &str;
}

/// Default no-op wake-lock. Suitable for headless CI runs and as a
/// safe fallback.
#[derive(Debug, Default)]
pub struct NoopWakeLock;

impl WakeLock for NoopWakeLock {
    fn acquire(&self, reason: WakeReason) -> Box<dyn WakeGuard> {
        Box::new(NoopGuard {
            label: format!("noop:{}", reason.summary),
        })
    }
}

struct NoopGuard {
    label: String,
}

impl WakeGuard for NoopGuard {
    fn label(&self) -> &str {
        &self.label
    }
}

/// Tracing-only backend: logs acquire + release at `debug`. Useful
/// during development to see which code paths are keeping the
/// machine awake.
#[derive(Debug, Default)]
pub struct TracingWakeLock;

impl WakeLock for TracingWakeLock {
    fn acquire(&self, reason: WakeReason) -> Box<dyn WakeGuard> {
        tracing::debug!(
            target: "vac_tui_runtime::prevent_sleep",
            summary = %reason.summary,
            "wake-lock acquired"
        );
        Box::new(TracingGuard {
            label: reason.summary,
        })
    }
}

struct TracingGuard {
    label: String,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        tracing::debug!(
            target: "vac_tui_runtime::prevent_sleep",
            summary = %self.label,
            "wake-lock released"
        );
    }
}

impl WakeGuard for TracingGuard {
    fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn noop_acquire_drop_is_infallible() {
        let lock = NoopWakeLock;
        let g = lock.acquire(WakeReason::new("build"));
        assert!(g.label().contains("build"));
    }

    struct CountingLock {
        acquired: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
    }

    struct CountingGuard {
        label: String,
        released: Arc<AtomicBool>,
    }

    impl Drop for CountingGuard {
        fn drop(&mut self) {
            self.released.store(true, Ordering::SeqCst);
        }
    }

    impl WakeGuard for CountingGuard {
        fn label(&self) -> &str {
            &self.label
        }
    }

    impl WakeLock for CountingLock {
        fn acquire(&self, reason: WakeReason) -> Box<dyn WakeGuard> {
            self.acquired.store(true, Ordering::SeqCst);
            Box::new(CountingGuard {
                label: reason.summary,
                released: self.released.clone(),
            })
        }
    }

    #[test]
    fn guard_drop_triggers_release() {
        let released = Arc::new(AtomicBool::new(false));
        let acquired = Arc::new(AtomicBool::new(false));
        let lock = CountingLock {
            acquired: acquired.clone(),
            released: released.clone(),
        };
        {
            let _g = lock.acquire(WakeReason::new("turn"));
            assert!(acquired.load(Ordering::SeqCst));
            assert!(!released.load(Ordering::SeqCst));
        }
        assert!(released.load(Ordering::SeqCst));
    }
}
