//! F7.6 — Rate-limit UX.
//!
//! When a provider signals throttling, we rotate through a small bank
//! of operator-facing messages so the banner doesn't feel stale.
//! Real rate-limit signals come from the HTTP 429 / provider-specific
//! error codes; this module owns the *display* side only.
//!
//! An env-var mock (`VAC_MOCK_RATE_LIMIT=1`) flips the state machine
//! into a forced-limit mode for screenshots + demos.

/// 10-message bank. Rotated round-robin; dedup with BannerQueue at
/// the call site if you don't want rapid-fire refreshes.
pub const RATE_LIMIT_MESSAGES: &[&str] = &[
    "provider throttled — easing off for a moment",
    "waiting for the model to catch its breath",
    "rate limit engaged — your prompt is queued",
    "back-pressure from upstream; retrying shortly",
    "provider asked for a pause; honoring their ask",
    "too many tokens per minute — slowing the tap",
    "429 received — backing off with jitter",
    "throttled. Compact history to reclaim headroom",
    "upstream is busy; next slot in a few seconds",
    "rate-limit banner rotating — operator may continue",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RateLimitKind {
    /// No limit active.
    None,
    /// Provider signalled soft back-pressure; keep submitting.
    Soft,
    /// Hard 429; engine should refuse submits until `retry_after`.
    Hard,
}

/// UI-facing rate-limit state. Updated by the engine when a provider
/// response carries rate-limit headers; read by the footer renderer.
#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub kind: RateLimitKind,
    pub retry_after_secs: Option<u32>,
    /// Which slot in [`RATE_LIMIT_MESSAGES`] is currently displayed.
    /// Advanced by [`Self::next_message`].
    pub message_idx: usize,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            kind: Self::initial_kind_from_env(),
            retry_after_secs: None,
            message_idx: 0,
        }
    }
}

impl RateLimitState {
    /// Current message text. Never panics even if message_idx drifts.
    pub fn current_message(&self) -> &'static str {
        if RATE_LIMIT_MESSAGES.is_empty() {
            return "";
        }
        let idx = self.message_idx % RATE_LIMIT_MESSAGES.len();
        RATE_LIMIT_MESSAGES[idx]
    }

    /// Advance to the next message in the bank. Returns the new text.
    pub fn next_message(&mut self) -> &'static str {
        self.message_idx = self.message_idx.wrapping_add(1);
        self.current_message()
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.kind, RateLimitKind::None)
    }

    /// Env-var mock: `VAC_MOCK_RATE_LIMIT=1` or `=soft` forces
    /// `Soft`, `=hard` forces `Hard`, anything else maps to `None`.
    /// Read on every call so that test harnesses can flip the env
    /// between `RateLimitState::default()` invocations — caching the
    /// first read in a `OnceLock` made tests in the same process
    /// observe stale values.
    fn initial_kind_from_env() -> RateLimitKind {
        match std::env::var("VAC_MOCK_RATE_LIMIT") {
            Ok(v) if v.eq_ignore_ascii_case("hard") => RateLimitKind::Hard,
            Ok(v) if v == "1" || v.eq_ignore_ascii_case("soft") => RateLimitKind::Soft,
            _ => RateLimitKind::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_bank_is_ten_entries() {
        assert_eq!(RATE_LIMIT_MESSAGES.len(), 10);
        assert!(RATE_LIMIT_MESSAGES.iter().all(|m| !m.is_empty()));
    }

    #[test]
    fn next_message_rotates_through_bank() {
        let mut s = RateLimitState {
            kind: RateLimitKind::Soft,
            retry_after_secs: None,
            message_idx: 0,
        };
        let first = s.current_message();
        let second = s.next_message();
        assert_ne!(first, second);
        // Walk a full cycle and expect to come back to `first`.
        for _ in 0..RATE_LIMIT_MESSAGES.len() - 1 {
            s.next_message();
        }
        assert_eq!(s.current_message(), first);
    }

    #[test]
    fn is_active_mirrors_kind() {
        let mut s = RateLimitState {
            kind: RateLimitKind::None,
            retry_after_secs: None,
            message_idx: 0,
        };
        assert!(!s.is_active());
        s.kind = RateLimitKind::Hard;
        assert!(s.is_active());
    }

    #[test]
    fn current_message_never_panics_on_wraparound() {
        let s = RateLimitState {
            kind: RateLimitKind::Soft,
            retry_after_secs: None,
            message_idx: usize::MAX,
        };
        let _ = s.current_message(); // must not panic
    }

    #[test]
    fn next_message_wraps_cleanly_at_usize_max() {
        let mut s = RateLimitState {
            kind: RateLimitKind::Soft,
            retry_after_secs: None,
            message_idx: usize::MAX,
        };
        let before = s.current_message();
        let after = s.next_message();
        // After wrapping_add(1) from usize::MAX we land on 0 →
        // the first message in the bank.
        assert_eq!(after, RATE_LIMIT_MESSAGES[0]);
        assert_ne!(before, after);
    }
}
