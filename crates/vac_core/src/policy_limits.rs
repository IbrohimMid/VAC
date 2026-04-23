//! W9.2 — org-level policy limits.
//!
//! `PolicyLimits` wraps configurable knobs that cap the session's
//! outbound traffic:
//!
//! - `max_submits_per_hour` — sliding 1-hour rate cap.
//! - `max_tokens_per_session` — total token budget across all
//!   submits in this session.
//! - `denied_tools` — tool names the policy blocklists outright.
//!
//! The file format is TOML. Default path is
//! `<project_root>/.vac/policy.toml`; an absent file yields
//! `PolicyLimits::unlimited()`. Env var `VAC_POLICY_PATH` overrides.
//!
//! Wired into `vac_session_engine::submit_one` via the
//! [`PolicyGate`] trait — callers hand the gate a `SubmitIntent`
//! (tool + token count); gate returns `PolicyDecision::Allow` or
//! `Deny(reason)`. The tracker maintains per-hour request counts +
//! per-session token totals in-memory.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

pub const DEFAULT_POLICY_FILENAME: &str = "policy.toml";
pub const POLICY_ENV_VAR: &str = "VAC_POLICY_PATH";
pub const POLICY_WINDOW_SECS: u64 = 3_600; // 1 hour

/// Parsed policy, straight off disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PolicyLimits {
    #[serde(default)]
    pub max_submits_per_hour: Option<u32>,
    #[serde(default)]
    pub max_tokens_per_session: Option<u64>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl PolicyLimits {
    /// Permissive — no caps, no blocklist. Used as fallback when the
    /// policy file is missing so fresh projects never block by
    /// accident.
    pub fn unlimited() -> Self {
        Self::default()
    }

    /// Load from `<project_root>/.vac/policy.toml`. Respects
    /// `VAC_POLICY_PATH` override. Missing file → `unlimited()`.
    /// Parse failures surface as `PolicyError::Parse` so the
    /// operator sees the typo; we never silently fall back on a
    /// broken config.
    pub async fn load(project_root: &Path) -> Result<Self, PolicyError> {
        let path = Self::resolve_path(project_root);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Self::unlimited());
        }
        let raw = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| PolicyError::Io(e.to_string()))?;
        toml::from_str(&raw).map_err(|e| PolicyError::Parse(e.to_string()))
    }

    pub fn resolve_path(project_root: &Path) -> PathBuf {
        if let Ok(p) = std::env::var(POLICY_ENV_VAR) {
            return PathBuf::from(p);
        }
        project_root.join(".vac").join(DEFAULT_POLICY_FILENAME)
    }

    /// Atomic save (temp + rename) so a crash mid-write leaves the
    /// prior policy intact. Used by `vac config set policy.*`
    /// (future) and the smoke-test fixtures.
    pub async fn save(&self, project_root: &Path) -> Result<(), PolicyError> {
        let path = Self::resolve_path(project_root);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| PolicyError::Io(e.to_string()))?;
        }
        let raw = toml::to_string(self).map_err(|e| PolicyError::Parse(e.to_string()))?;
        let tmp = path.with_extension("toml.tmp");
        tokio::fs::write(&tmp, raw)
            .await
            .map_err(|e| PolicyError::Io(e.to_string()))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .map_err(|e| PolicyError::Io(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum PolicyError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "policy io: {m}"),
            Self::Parse(m) => write!(f, "policy parse: {m}"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// What the caller hands the gate before each submit / tool use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitIntent<'a> {
    pub tool: Option<&'a str>,
    pub additional_tokens: u64,
}

/// What the gate returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
}

/// Runtime tracker — loaded policy + session-lifetime counters.
/// Cheap to clone via Arc.
pub struct PolicyTracker {
    policy: PolicyLimits,
    state: Arc<Mutex<TrackerState>>,
    clock: Arc<dyn Clock>,
}

impl Clone for PolicyTracker {
    fn clone(&self) -> Self {
        Self {
            policy: self.policy.clone(),
            state: self.state.clone(),
            clock: self.clock.clone(),
        }
    }
}

#[derive(Default)]
struct TrackerState {
    submit_timestamps: VecDeque<u64>,
    tokens_consumed: u64,
}

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

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

impl PolicyTracker {
    pub fn new(policy: PolicyLimits) -> Self {
        Self::with_clock(policy, Arc::new(SystemClock))
    }

    pub fn with_clock(policy: PolicyLimits, clock: Arc<dyn Clock>) -> Self {
        Self {
            policy,
            state: Arc::new(Mutex::new(TrackerState::default())),
            clock,
        }
    }

    pub fn policy(&self) -> &PolicyLimits {
        &self.policy
    }

    /// Check an intent against the policy. Does NOT mutate — the
    /// caller follows up with `record_submit` on a successful
    /// allow, `record_tokens` after the LLM response.
    ///
    /// Zero-cap semantics: `max_submits_per_hour=0` blocks every
    /// submit (legitimate freeze policy), `max_tokens_per_session=0`
    /// blocks any submit that would consume > 0 tokens. Operators
    /// who want "unlimited" must leave the field `None`, NOT `0`.
    pub async fn check(&self, intent: &SubmitIntent<'_>) -> PolicyDecision {
        // Denied-tool check short-circuits regardless of counters.
        // Match is case-insensitive to mirror the MCP channel ACL
        // (W4.2 audit fix) — `denied_tools=["Bash"]` rejects `bash`.
        if let Some(tool) = intent.tool {
            let needle = tool.to_ascii_lowercase();
            if self
                .policy
                .denied_tools
                .iter()
                .any(|t| t.to_ascii_lowercase() == needle)
            {
                let decision = PolicyDecision::Deny(format!(
                    "tool {tool} is denied by policy"
                ));
                trace_deny(&decision, "denied_tools");
                return decision;
            }
        }
        let now = unix_secs(self.clock.now());
        let guard = self.state.lock().await;

        if let Some(max) = self.policy.max_submits_per_hour {
            let visible: usize = guard
                .submit_timestamps
                .iter()
                .rev()
                .take_while(|t| now.saturating_sub(**t) < POLICY_WINDOW_SECS)
                .count();
            if visible as u32 >= max {
                let decision = PolicyDecision::Deny(format!(
                    "max_submits_per_hour ({max}) exceeded"
                ));
                trace_deny(&decision, "max_submits_per_hour");
                return decision;
            }
        }
        if let Some(max_tokens) = self.policy.max_tokens_per_session {
            let projected = guard
                .tokens_consumed
                .saturating_add(intent.additional_tokens);
            if projected > max_tokens {
                let decision = PolicyDecision::Deny(format!(
                    "max_tokens_per_session ({max_tokens}) would be exceeded ({projected})"
                ));
                trace_deny(&decision, "max_tokens_per_session");
                return decision;
            }
        }
        PolicyDecision::Allow
    }

    /// Record a submit after the gate has allowed it. Updates the
    /// sliding window.
    pub async fn record_submit(&self) {
        let now = unix_secs(self.clock.now());
        let mut guard = self.state.lock().await;
        guard.submit_timestamps.push_back(now);
        prune_window(&mut guard.submit_timestamps, now);
    }

    /// Record tokens consumed for a completed submit. Separate call
    /// because the token count is only known after the LLM response.
    pub async fn record_tokens(&self, n: u64) {
        let mut guard = self.state.lock().await;
        guard.tokens_consumed = guard.tokens_consumed.saturating_add(n);
    }

    pub async fn snapshot(&self) -> PolicySnapshot {
        let now = unix_secs(self.clock.now());
        let guard = self.state.lock().await;
        let recent = guard
            .submit_timestamps
            .iter()
            .rev()
            .take_while(|t| now.saturating_sub(**t) < POLICY_WINDOW_SECS)
            .count();
        PolicySnapshot {
            submits_last_hour: recent as u32,
            tokens_consumed: guard.tokens_consumed,
            policy: self.policy.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub submits_last_hour: u32,
    pub tokens_consumed: u64,
    pub policy: PolicyLimits,
}

/// Single place to emit a trace event when a policy check denies.
/// Keeps the call sites terse and means operators can filter on
/// `target=vac_core::policy_limits level=warn` to find every rejection.
fn trace_deny(decision: &PolicyDecision, rule: &'static str) {
    if let PolicyDecision::Deny(reason) = decision {
        tracing::warn!(
            target: "vac_core::policy_limits",
            rule = rule,
            reason = %reason,
            "policy denied submit intent",
        );
    }
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn prune_window(q: &mut VecDeque<u64>, now: u64) {
    while let Some(front) = q.front() {
        if now.saturating_sub(*front) >= POLICY_WINDOW_SECS {
            q.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(tool: Option<&str>, tokens: u64) -> SubmitIntent<'_> {
        SubmitIntent {
            tool,
            additional_tokens: tokens,
        }
    }

    #[tokio::test]
    async fn unlimited_policy_always_allows() {
        let t = PolicyTracker::new(PolicyLimits::unlimited());
        for _ in 0..100 {
            assert_eq!(t.check(&intent(None, 100)).await, PolicyDecision::Allow);
            t.record_submit().await;
        }
    }

    #[tokio::test]
    async fn max_submits_per_hour_gate() {
        // Plan's acceptance: max_submits_per_hour = 3 → 4th rejected.
        let policy = PolicyLimits {
            max_submits_per_hour: Some(3),
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        for _ in 0..3 {
            assert_eq!(t.check(&intent(None, 0)).await, PolicyDecision::Allow);
            t.record_submit().await;
        }
        let denied = t.check(&intent(None, 0)).await;
        match denied {
            PolicyDecision::Deny(r) => assert!(r.contains("max_submits_per_hour")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn submit_window_slides_after_one_hour() {
        let clock = Arc::new(FakeClock::new());
        let policy = PolicyLimits {
            max_submits_per_hour: Some(2),
            ..Default::default()
        };
        let t = PolicyTracker::with_clock(policy, clock.clone());
        t.record_submit().await;
        t.record_submit().await;
        assert!(matches!(
            t.check(&intent(None, 0)).await,
            PolicyDecision::Deny(_)
        ));
        clock.advance(POLICY_WINDOW_SECS + 1);
        assert_eq!(t.check(&intent(None, 0)).await, PolicyDecision::Allow);
    }

    #[tokio::test]
    async fn max_tokens_per_session_gate() {
        let policy = PolicyLimits {
            max_tokens_per_session: Some(1_000),
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        assert_eq!(t.check(&intent(None, 500)).await, PolicyDecision::Allow);
        t.record_tokens(500).await;
        assert_eq!(t.check(&intent(None, 500)).await, PolicyDecision::Allow);
        t.record_tokens(500).await;
        match t.check(&intent(None, 1)).await {
            PolicyDecision::Deny(r) => assert!(r.contains("max_tokens_per_session")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn denied_tool_blocked_even_with_headroom() {
        let policy = PolicyLimits {
            max_submits_per_hour: Some(100),
            denied_tools: vec!["bash".into()],
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        match t.check(&intent(Some("bash"), 0)).await {
            PolicyDecision::Deny(r) => assert!(r.contains("bash")),
            other => panic!("expected Deny, got {other:?}"),
        }
        // A non-denied tool still passes.
        assert_eq!(
            t.check(&intent(Some("grep"), 0)).await,
            PolicyDecision::Allow
        );
    }

    #[tokio::test]
    async fn denied_tools_case_insensitive() {
        // Post-audit: match the MCP channel ACL (W4.2) semantics.
        let policy = PolicyLimits {
            denied_tools: vec!["Bash".into()],
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        // Policy spelled `Bash`; invocation arrives as `bash`.
        match t.check(&intent(Some("bash"), 0)).await {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected Deny, got {other:?}"),
        }
        // Explicit uppercase also denies.
        match t.check(&intent(Some("BASH"), 0)).await {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn zero_cap_submits_blocks_everything() {
        // Documents the freeze-policy semantics — a typo or
        // deliberate freeze of the tenant.
        let policy = PolicyLimits {
            max_submits_per_hour: Some(0),
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        match t.check(&intent(None, 0)).await {
            PolicyDecision::Deny(r) => assert!(r.contains("max_submits_per_hour")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn zero_cap_tokens_blocks_nonzero_intent() {
        let policy = PolicyLimits {
            max_tokens_per_session: Some(0),
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        // Zero-token intent passes (trivially); positive denies.
        assert_eq!(t.check(&intent(None, 0)).await, PolicyDecision::Allow);
        match t.check(&intent(None, 1)).await {
            PolicyDecision::Deny(r) => assert!(r.contains("max_tokens_per_session")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_missing_file_returns_unlimited() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = PolicyLimits::load(tmp.path()).await.unwrap();
        assert_eq!(policy, PolicyLimits::unlimited());
    }

    #[tokio::test]
    async fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let want = PolicyLimits {
            max_submits_per_hour: Some(5),
            max_tokens_per_session: Some(42_000),
            denied_tools: vec!["bash".into(), "file_write".into()],
        };
        want.save(tmp.path()).await.unwrap();
        let back = PolicyLimits::load(tmp.path()).await.unwrap();
        assert_eq!(want, back);
    }

    #[tokio::test]
    async fn save_is_atomic_no_temp_left_over() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = PolicyLimits {
            max_submits_per_hour: Some(10),
            ..Default::default()
        };
        policy.save(tmp.path()).await.unwrap();
        let parent = tmp.path().join(".vac");
        let mut rd = tokio::fs::read_dir(&parent).await.unwrap();
        while let Some(e) = rd.next_entry().await.unwrap() {
            let s = e.file_name().to_string_lossy().into_owned();
            assert!(!s.ends_with(".tmp"), "leftover temp: {s}");
        }
    }

    #[tokio::test]
    async fn parse_error_surfaces_not_swallowed() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".vac").join(DEFAULT_POLICY_FILENAME);
        tokio::fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&path, "max_submits_per_hour = \"nope\"").await.unwrap();
        let err = PolicyLimits::load(tmp.path()).await.unwrap_err();
        matches!(err, PolicyError::Parse(_));
    }

    #[tokio::test]
    async fn snapshot_reflects_counters() {
        let policy = PolicyLimits {
            max_submits_per_hour: Some(10),
            max_tokens_per_session: Some(10_000),
            ..Default::default()
        };
        let t = PolicyTracker::new(policy);
        t.record_submit().await;
        t.record_submit().await;
        t.record_tokens(1234).await;
        let snap = t.snapshot().await;
        assert_eq!(snap.submits_last_hour, 2);
        assert_eq!(snap.tokens_consumed, 1234);
    }

    #[tokio::test]
    async fn env_var_override_takes_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let alt = tmp.path().join("alt.toml");
        let policy = PolicyLimits {
            max_submits_per_hour: Some(99),
            ..Default::default()
        };
        tokio::fs::write(&alt, toml::to_string(&policy).unwrap())
            .await
            .unwrap();
        let prior = std::env::var_os(POLICY_ENV_VAR);
        unsafe {
            std::env::set_var(POLICY_ENV_VAR, alt.to_str().unwrap());
        }
        let fresh_root = tempfile::tempdir().unwrap();
        let loaded = PolicyLimits::load(fresh_root.path()).await.unwrap();
        match prior {
            Some(v) => unsafe { std::env::set_var(POLICY_ENV_VAR, v) },
            None => unsafe { std::env::remove_var(POLICY_ENV_VAR) },
        }
        assert_eq!(loaded.max_submits_per_hour, Some(99));
    }
}
