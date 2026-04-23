//! W9.1 — per-provider rate-limit tracker with jittered backoff.
//!
//! Upstream providers (Anthropic, OpenAI, Gemini, xAI, Mistral,
//! openai-compat) enforce per-key RPM / TPM ceilings. When a
//! request returns 429 we need to:
//!
//! 1. Respect the server's `Retry-After` header (seconds).
//! 2. Add jitter so N VAC processes sharing a key don't retry in
//!    lockstep after a shared cooldown.
//! 3. Keep a sliding-window RPM counter so the router can steer
//!    load between providers before anyone trips the 429.
//!
//! Shape:
//!
//! - `RateLimitTracker` — cheap-to-clone handle (`Arc<Mutex<…>>`).
//!   One tracker per `LlmRouter` instance.
//! - `observe_request(provider)` — call before each outbound
//!   request; updates the 60-second RPM window.
//! - `observe_429(provider, retry_after)` — call on 429 response.
//!   Returns the `Duration` the caller should sleep before retrying
//!   (server's hint + jitter).
//! - `remaining_rpm(provider, ceiling)` — projected headroom in the
//!   current window.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

pub const RPM_WINDOW_SECS: u64 = 60;

/// How much random jitter the tracker sprinkles on every backoff.
/// Expressed as a fraction of the server-hinted wait — a hint of
/// 30 s with `JITTER_FRACTION=0.1` yields actual sleeps between
/// 30.0 and 33.0 s. Jitter is one-sided (always adds) so no caller
/// ever undercuts the server's instruction.
pub const JITTER_FRACTION: f64 = 0.1;

/// Hard ceiling on the total backoff we'll report, to prevent a
/// hostile / buggy `Retry-After` header from wedging the router
/// for hours.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// Minimum backoff applied on a 429 even when the server doesn't
/// include `Retry-After`. Five seconds gives enough margin to avoid
/// an immediate second strike while keeping the UX responsive.
pub const DEFAULT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Default)]
struct ProviderWindow {
    /// Monotonic request timestamps (unix seconds). Oldest first.
    requests: VecDeque<u64>,
    /// When the last observed 429 expires; requests before this
    /// instant should still be treated as rate-limited.
    cooldown_until: Option<SystemTime>,
}

/// Trait so tests can pin "now" without mocking wall-clock.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

/// Production clock — `SystemTime::now()`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Deterministic clock for tests — seconds since construction.
pub struct FakeClock {
    base: SystemTime,
    offset: Arc<std::sync::atomic::AtomicU64>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            base: SystemTime::now(),
            offset: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn advance(&self, secs: u64) {
        self.offset
            .fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        self.base
            + Duration::from_secs(
                self.offset.load(std::sync::atomic::Ordering::SeqCst),
            )
    }
}

/// Per-provider rate-limit state. Clone is cheap (Arc refcount).
pub struct RateLimitTracker {
    inner: Arc<Mutex<HashMap<String, ProviderWindow>>>,
    clock: Arc<dyn Clock>,
    /// Jitter-RNG seed bumped on every call so consecutive jitters
    /// are different without needing a real RNG crate.
    jitter_seed: Arc<std::sync::atomic::AtomicU64>,
}

impl Clone for RateLimitTracker {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            clock: self.clock.clone(),
            jitter_seed: self.jitter_seed.clone(),
        }
    }
}

impl Default for RateLimitTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitTracker {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            clock,
            jitter_seed: Arc::new(std::sync::atomic::AtomicU64::new(0xA17E_BA5E)),
        }
    }

    /// Record that a request is about to go out. Updates the sliding
    /// window; does not gate the request — the caller consults
    /// `remaining_rpm` / `backoff_remaining` before deciding.
    pub async fn observe_request(&self, provider: &str) {
        let now = unix_secs(self.clock.now());
        let mut guard = self.inner.lock().await;
        let entry = guard
            .entry(provider.to_string())
            .or_default();
        entry.requests.push_back(now);
        prune_window(&mut entry.requests, now);
    }

    /// Record a 429 response. `retry_after` is the server-hinted
    /// sleep, or `None` when the response didn't include the header.
    /// Returns the effective backoff (server hint + jitter), with
    /// the **final** sleep clamped to `MAX_BACKOFF` so a hostile
    /// header plus jitter can never drift past the ceiling.
    pub async fn observe_429(
        &self,
        provider: &str,
        retry_after: Option<Duration>,
    ) -> Duration {
        let base = retry_after.unwrap_or(DEFAULT_BACKOFF);
        let base = base.min(MAX_BACKOFF);
        let jitter = self.jitter_for(base);
        // Clamp AFTER adding jitter so the documented invariant —
        // "effective backoff ≤ MAX_BACKOFF" — is actually held.
        let effective = base.saturating_add(jitter).min(MAX_BACKOFF);
        let cooldown_until = self.clock.now() + effective;
        let mut guard = self.inner.lock().await;
        let entry = guard.entry(provider.to_string()).or_default();
        entry.cooldown_until = Some(cooldown_until);
        tracing::debug!(
            target: "vil_llm::rate_limit",
            provider = provider,
            backoff_ms = effective.as_millis() as u64,
            "rate-limit cooldown applied",
        );
        effective
    }

    /// How many seconds remain until the provider leaves cooldown.
    /// `Duration::ZERO` when no cooldown is active.
    pub async fn backoff_remaining(&self, provider: &str) -> Duration {
        let now = self.clock.now();
        let guard = self.inner.lock().await;
        let Some(entry) = guard.get(provider) else {
            return Duration::ZERO;
        };
        match entry.cooldown_until {
            Some(deadline) => deadline
                .duration_since(now)
                .unwrap_or(Duration::ZERO),
            None => Duration::ZERO,
        }
    }

    /// Projected remaining RPM against `ceiling`. Returns
    /// `ceiling - window_len`, saturating at zero.
    pub async fn remaining_rpm(&self, provider: &str, ceiling: u32) -> u32 {
        let now = unix_secs(self.clock.now());
        let guard = self.inner.lock().await;
        let Some(entry) = guard.get(provider) else {
            return ceiling;
        };
        // Prune for the caller's read — avoids reporting stale count
        // when nothing has written since the window rolled.
        let window_len = entry
            .requests
            .iter()
            .rev()
            .take_while(|t| now.saturating_sub(**t) < RPM_WINDOW_SECS)
            .count();
        ceiling.saturating_sub(window_len as u32)
    }

    /// Snapshot for telemetry. Returns `(provider, current_rpm,
    /// backoff_secs)` triples.
    pub async fn snapshot(&self) -> Vec<(String, u32, u64)> {
        let now = unix_secs(self.clock.now());
        let guard = self.inner.lock().await;
        let now_sys = self.clock.now();
        guard
            .iter()
            .map(|(name, w)| {
                let rpm = w
                    .requests
                    .iter()
                    .rev()
                    .take_while(|t| now.saturating_sub(**t) < RPM_WINDOW_SECS)
                    .count() as u32;
                let backoff = w
                    .cooldown_until
                    .and_then(|c| c.duration_since(now_sys).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (name.clone(), rpm, backoff)
            })
            .collect()
    }

    /// Clear all tracked state. Used in tests + on router shutdown.
    pub async fn reset(&self) {
        self.inner.lock().await.clear();
    }

    /// One-sided deterministic jitter derived from the rolling seed.
    /// Falls in `0 ..= JITTER_FRACTION * base`.
    fn jitter_for(&self, base: Duration) -> Duration {
        // Bump + mix — a tiny xorshift keeps consecutive jitters
        // distinct without pulling a rand dep into the crate.
        let seed = self.jitter_seed
            .fetch_add(0x9E37_79B9_7F4A_7C15, std::sync::atomic::Ordering::SeqCst);
        let mixed = xorshift64(seed.wrapping_add(1));
        let frac = (mixed as f64) / (u64::MAX as f64); // 0..=1
        let base_ms = base.as_millis() as f64;
        let jitter_ms = (base_ms * JITTER_FRACTION * frac) as u64;
        Duration::from_millis(jitter_ms)
    }
}

fn xorshift64(mut x: u64) -> u64 {
    if x == 0 {
        x = 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn prune_window(q: &mut VecDeque<u64>, now: u64) {
    while let Some(front) = q.front() {
        if now.saturating_sub(*front) >= RPM_WINDOW_SECS {
            q.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_tracker_reports_full_rpm_headroom() {
        let t = RateLimitTracker::new();
        assert_eq!(t.remaining_rpm("openai", 100).await, 100);
        assert_eq!(t.backoff_remaining("openai").await, Duration::ZERO);
    }

    #[tokio::test]
    async fn observe_request_consumes_rpm() {
        let t = RateLimitTracker::new();
        for _ in 0..5 {
            t.observe_request("openai").await;
        }
        assert_eq!(t.remaining_rpm("openai", 10).await, 5);
    }

    #[tokio::test]
    async fn sliding_window_evicts_old_entries() {
        let clock = Arc::new(FakeClock::new());
        let t = RateLimitTracker::with_clock(clock.clone());
        t.observe_request("openai").await;
        t.observe_request("openai").await;
        // Fast-forward past the 60-s window.
        clock.advance(RPM_WINDOW_SECS + 1);
        assert_eq!(t.remaining_rpm("openai", 10).await, 10);
    }

    #[tokio::test]
    async fn observe_429_sets_cooldown() {
        let clock = Arc::new(FakeClock::new());
        let t = RateLimitTracker::with_clock(clock.clone());
        let waited = t
            .observe_429("openai", Some(Duration::from_secs(30)))
            .await;
        // 30 s base, up to +10% jitter → 30..=33 s.
        assert!(waited >= Duration::from_secs(30));
        assert!(waited <= Duration::from_millis(33_000));
        let remaining = t.backoff_remaining("openai").await;
        assert!(remaining > Duration::from_secs(29));
    }

    #[tokio::test]
    async fn observe_429_without_hint_uses_default() {
        let t = RateLimitTracker::new();
        let waited = t.observe_429("openai", None).await;
        assert!(waited >= DEFAULT_BACKOFF);
        assert!(waited <= DEFAULT_BACKOFF + Duration::from_millis(
            ((DEFAULT_BACKOFF.as_millis() as f64) * JITTER_FRACTION) as u64 + 1,
        ));
    }

    #[tokio::test]
    async fn backoff_is_clamped_to_max_including_jitter() {
        let t = RateLimitTracker::new();
        // Hostile header says "come back in a day" — post-audit we
        // clamp the FINAL effective backoff to MAX_BACKOFF so jitter
        // can't drift past it.
        for _ in 0..50 {
            let waited = t
                .observe_429("openai", Some(Duration::from_secs(86400)))
                .await;
            assert!(
                waited <= MAX_BACKOFF,
                "effective backoff must be <= MAX_BACKOFF: {waited:?}",
            );
        }
    }

    #[tokio::test]
    async fn cooldown_expires_with_clock() {
        let clock = Arc::new(FakeClock::new());
        let t = RateLimitTracker::with_clock(clock.clone());
        t.observe_429("openai", Some(Duration::from_secs(10))).await;
        assert!(t.backoff_remaining("openai").await > Duration::ZERO);
        clock.advance(60);
        assert_eq!(t.backoff_remaining("openai").await, Duration::ZERO);
    }

    #[tokio::test]
    async fn jitter_is_one_sided_never_undercuts_server() {
        // Run many samples; every effective backoff must be >= base.
        let t = RateLimitTracker::new();
        let base = Duration::from_secs(30);
        for _ in 0..200 {
            // Use internal helper on a fresh tracker clone so the
            // rolling seed advances deterministically.
            let j = t.jitter_for(base);
            assert!(j <= Duration::from_secs(3), "jitter exceeds 10%: {j:?}");
            assert!(base + j >= base);
        }
    }

    #[tokio::test]
    async fn providers_isolated() {
        let t = RateLimitTracker::new();
        t.observe_429("openai", Some(Duration::from_secs(10))).await;
        assert!(t.backoff_remaining("openai").await > Duration::ZERO);
        assert_eq!(t.backoff_remaining("anthropic").await, Duration::ZERO);
    }

    #[tokio::test]
    async fn snapshot_reflects_state() {
        let t = RateLimitTracker::new();
        t.observe_request("openai").await;
        t.observe_request("openai").await;
        t.observe_429("openai", Some(Duration::from_secs(5))).await;
        let snap = t.snapshot().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, "openai");
        assert_eq!(snap[0].1, 2);
        assert!(snap[0].2 >= 1);
    }

    #[tokio::test]
    async fn reset_clears_all_state() {
        let t = RateLimitTracker::new();
        t.observe_request("openai").await;
        t.observe_429("openai", Some(Duration::from_secs(10))).await;
        t.reset().await;
        assert_eq!(t.remaining_rpm("openai", 10).await, 10);
        assert_eq!(t.backoff_remaining("openai").await, Duration::ZERO);
    }

    #[tokio::test]
    async fn consecutive_jitters_differ() {
        let t = RateLimitTracker::new();
        let base = Duration::from_secs(30);
        let mut seen: std::collections::HashSet<u128> =
            std::collections::HashSet::new();
        for _ in 0..20 {
            seen.insert(t.jitter_for(base).as_millis());
        }
        // Seeded xorshift should produce ≥ 15 distinct jitters in
        // 20 samples — covers the "constant jitter" regression.
        assert!(seen.len() >= 15);
    }
}
