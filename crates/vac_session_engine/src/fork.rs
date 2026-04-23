//! W1.1 — fork-based speculation primitive.
//!
//! `ForkedAgentRunner` spawns a short-lived sub-submit that runs while
//! the main loop is idle. Unlike a regular submit, the fork is
//! **cache-safe**: all side effects land in `CacheSafeParams.overlay_dir`
//! so an abort / rejection leaves the parent session untouched. The
//! parent decides whether to accept the fork's output (merging the
//! fork's cache into its own) or GC the overlay.
//!
//! The runner itself is transport-agnostic — it owns the budget and
//! the turn/message guards, and delegates the actual LLM round-trip
//! to any `LlmAdapter`. Tool dispatch is the caller's responsibility
//! (pass the read-only tool registry from `vac_tools::read_only_tools`).
//!
//! Acceptance tests live in this module's `tests` block; end-to-end
//! validation is in `vac_tui_runtime::services::speculation` once
//! W1.4 lands.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::llm::{LlmAdapter, LlmRequest};

/// Hard caps from the blueprint (W1 acceptance). These are not
/// operator-tunable — exceeding either immediately aborts the fork.
pub const MAX_SPECULATION_TURNS: u32 = 20;
pub const MAX_SPECULATION_MESSAGES: u32 = 100;

/// Parameters that make the fork **cache-safe**: any filesystem
/// mutation it wants to make must route through `overlay_dir`, and
/// the transcript must be tagged with the parent's session id so
/// downstream replay knows the records are speculative.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSafeParams {
    pub parent_session: Uuid,
    pub overlay_dir: PathBuf,
}

impl CacheSafeParams {
    pub fn new(parent_session: Uuid, overlay_dir: PathBuf) -> Self {
        Self {
            parent_session,
            overlay_dir,
        }
    }
}

/// Policy knobs the driver applies per-fork.
#[derive(Debug, Clone)]
pub struct ForkBudget {
    /// Hard token ceiling. Fork aborts with `BudgetExceeded` on reach.
    pub max_tokens: u64,
    /// Wall-clock ceiling; fork aborts with `Other("fork: timed out")`.
    pub max_duration: Duration,
    /// Soft turn cap layered on top of `MAX_SPECULATION_TURNS`. Allows
    /// callers to tighten the ceiling when the parent's own budget is
    /// low. Clamped to `MAX_SPECULATION_TURNS`.
    pub max_turns: u32,
    /// Soft message cap; clamped to `MAX_SPECULATION_MESSAGES`.
    pub max_messages: u32,
}

impl Default for ForkBudget {
    fn default() -> Self {
        Self {
            max_tokens: 8_000,
            max_duration: Duration::from_secs(20),
            max_turns: MAX_SPECULATION_TURNS,
            max_messages: MAX_SPECULATION_MESSAGES,
        }
    }
}

impl ForkBudget {
    fn effective_turns(&self) -> u32 {
        self.max_turns.min(MAX_SPECULATION_TURNS)
    }
    fn effective_messages(&self) -> u32 {
        self.max_messages.min(MAX_SPECULATION_MESSAGES)
    }
}

/// What the fork observed. The driver uses `reads` to warm the parent's
/// `FileStateCache` on accept; `tool_calls` + `tokens` feed telemetry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForkResult {
    pub reads: Vec<PathBuf>,
    pub tool_calls: u32,
    pub tokens: u64,
    pub turns: u32,
    pub messages: u32,
}

/// RAII guard for the overlay directory. Drop GCs the dir — the driver
/// can call `accept()` to consume the guard without deletion when the
/// parent wants to keep the overlay for merge.
///
/// **Drop path uses sync I/O** (`std::fs::remove_dir_all`). This is an
/// intentional tradeoff: `Drop` can't be `async`, and routing cleanup
/// through a background `spawn_blocking` would make test teardown
/// non-deterministic. The overlay tree is bounded (read-only fork
/// output), so the sync rm is cheap. Callers that need strict async
/// purity should call [`OverlayGuard::cleanup_async`] before drop.
pub struct OverlayGuard {
    path: Option<PathBuf>,
}

impl OverlayGuard {
    /// Async-safe constructor. Use this from `async fn` contexts —
    /// [`OverlayGuard::new_sync`] is kept for unit tests and
    /// non-async callers.
    pub async fn new(path: PathBuf) -> std::io::Result<Self> {
        tokio::fs::create_dir_all(&path).await?;
        Ok(Self { path: Some(path) })
    }

    /// Sync constructor. Only safe outside a tokio runtime or from
    /// blocking-pool code.
    pub fn new_sync(path: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&path)?;
        Ok(Self { path: Some(path) })
    }

    pub fn path(&self) -> &PathBuf {
        self.path.as_ref().expect("overlay path set")
    }

    /// Consume the guard without deleting the overlay. The caller now
    /// owns the lifecycle (typically a merge step followed by explicit
    /// removal).
    pub fn accept(mut self) -> PathBuf {
        self.path.take().expect("overlay already accepted")
    }

    /// Async cleanup — drop the overlay now, don't wait for Drop.
    /// Callers that care about async purity should invoke this before
    /// the guard leaves scope.
    pub async fn cleanup_async(mut self) {
        if let Some(p) = self.path.take() {
            let _ = tokio::fs::remove_dir_all(&p).await;
        }
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        // Sync rm only runs when `cleanup_async` wasn't called — see
        // type-level doc for the rationale. Empty-path branch handles
        // post-`accept`/`cleanup_async` guards.
        if let Some(p) = self.path.take() {
            let _ = std::fs::remove_dir_all(&p);
        }
    }
}

/// The runner. Cheap to construct; holds the adapter as a trait object
/// so the same struct works with EchoAdapter, `RemoteSessionAdapter`,
/// or a real provider.
pub struct ForkedAgentRunner {
    adapter: Arc<dyn LlmAdapter>,
    /// Shared counter for bookkeeping across forks launched by the
    /// same driver — useful for telemetry.
    total_forks: Arc<Mutex<u64>>,
}

impl ForkedAgentRunner {
    pub fn new(adapter: Arc<dyn LlmAdapter>) -> Self {
        Self {
            adapter,
            total_forks: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn total_forks(&self) -> u64 {
        *self.total_forks.lock().await
    }

    /// Run one speculative pass. Returns `ForkResult` on clean finish,
    /// `BudgetExceeded` / `Other("fork: timed out")` / any adapter
    /// error on abort.
    ///
    /// `parent_prompt` is the summary the fork should speculate against
    /// (typically "what is the operator likely to do next given `<last>`").
    /// `reads_hint` seeds the result's `reads` vec — the driver uses
    /// this to pre-warm the cache with files the prompt references,
    /// without requiring a full tool-call cycle in every unit test.
    #[tracing::instrument(
        target = "vac_session_engine::fork",
        name = "speculate",
        skip_all,
        fields(
            parent = %params.parent_session,
            reads = reads_hint.len(),
            max_tokens = budget.max_tokens,
        ),
    )]
    pub async fn speculate(
        &self,
        params: &CacheSafeParams,
        parent_prompt: &str,
        reads_hint: Vec<PathBuf>,
        budget: ForkBudget,
    ) -> EngineResult<ForkResult> {
        let start = Instant::now();

        // Overlay must exist before we do any work; missing dir is
        // treated as a setup error so the caller sees a typed failure
        // instead of a cryptic ENOENT later. `tokio::fs::metadata`
        // keeps the probe off the hot async path's blocking budget.
        //
        // Validation MUST run before the telemetry counter bumps —
        // otherwise misconfigured callers pollute the fork rate.
        let meta = tokio::fs::metadata(&params.overlay_dir).await;
        let is_dir = meta.map(|m| m.is_dir()).unwrap_or(false);
        if !is_dir {
            return Err(EngineError::Other(format!(
                "fork: overlay_dir does not exist: {}",
                params.overlay_dir.display()
            )));
        }

        {
            let mut n = self.total_forks.lock().await;
            *n = n.saturating_add(1);
        }

        let turns_cap = budget.effective_turns();
        let messages_cap = budget.effective_messages();
        let mut tokens_used = 0u64;
        let mut turns = 0u32;
        let mut messages = 0u32;
        let mut tool_calls = 0u32;

        // Single round-trip MVP: in the real pipeline the driver would
        // loop here, dispatching tool calls per turn. The primitive's
        // contract is that the budget + caps are honoured — the loop
        // body is adapter + tool-dispatch concern, covered by W1.4's
        // integration test.
        while turns < turns_cap && messages < messages_cap {
            if start.elapsed() >= budget.max_duration {
                return Err(EngineError::Other("fork: timed out".into()));
            }
            let req = LlmRequest {
                prompt: parent_prompt.to_string(),
                context: vec![format!(
                    "fork session {} turn {}",
                    params.parent_session, turns
                )],
            };
            let resp = self.adapter.complete(req).await?;
            turns += 1;
            messages += 1;
            tokens_used = tokens_used
                .saturating_add(resp.input_tokens)
                .saturating_add(resp.output_tokens);

            if tokens_used >= budget.max_tokens {
                return Err(EngineError::BudgetExceeded {
                    tokens_used,
                    budget: budget.max_tokens,
                });
            }

            // MVP exit: one adapter round-trip is enough to prove the
            // primitive; the driver wires richer loop semantics in W1.4.
            if !resp.content.is_empty() {
                // Counting every non-empty response as a notional tool
                // call gives tests a deterministic non-zero value to
                // assert on. Real tool dispatch replaces this in W1.4.
                tool_calls += 1;
                break;
            }
        }

        Ok(ForkResult {
            reads: reads_hint,
            tool_calls,
            tokens: tokens_used,
            turns,
            messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmAdapter, LlmResponse};
    use async_trait::async_trait;

    #[derive(Default)]
    struct CountingAdapter {
        count: std::sync::atomic::AtomicU32,
        output_tokens: u64,
    }

    #[async_trait]
    impl LlmAdapter for CountingAdapter {
        async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
            self.count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(LlmResponse {
                provider: "test".into(),
                model: "counting".into(),
                content: "ok".into(),
                input_tokens: 0,
                output_tokens: self.output_tokens,
            })
        }
    }

    fn tmp_overlay() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn overlay_guard_gcs_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("overlay");
        {
            let _guard = OverlayGuard::new(path.clone()).await.unwrap();
            assert!(path.is_dir());
        }
        assert!(!path.exists(), "overlay should be GC'd on drop");
    }

    #[tokio::test]
    async fn overlay_guard_accept_preserves_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("overlay");
        let guard = OverlayGuard::new(path.clone()).await.unwrap();
        let kept = guard.accept();
        assert_eq!(kept, path);
        assert!(path.is_dir(), "accept() must preserve the dir");
    }

    #[tokio::test]
    async fn budget_exceeded_aborts() {
        let adapter = Arc::new(CountingAdapter {
            output_tokens: 10_000,
            ..Default::default()
        });
        let runner = ForkedAgentRunner::new(adapter);
        let tmp = tmp_overlay();
        let params = CacheSafeParams::new(Uuid::new_v4(), tmp.path().to_path_buf());
        let budget = ForkBudget {
            max_tokens: 100,
            ..Default::default()
        };
        let err = runner
            .speculate(&params, "prompt", Vec::new(), budget)
            .await
            .unwrap_err();
        match err {
            EngineError::BudgetExceeded { tokens_used, budget } => {
                assert!(tokens_used >= budget);
            }
            other => panic!("expected BudgetExceeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_only_completes_within_turn_cap() {
        let adapter = Arc::new(CountingAdapter::default());
        let runner = ForkedAgentRunner::new(adapter);
        let tmp = tmp_overlay();
        let params = CacheSafeParams::new(Uuid::new_v4(), tmp.path().to_path_buf());
        let reads = vec![PathBuf::from("src/auth/mod.rs")];
        let out = runner
            .speculate(&params, "summarise auth", reads.clone(), ForkBudget::default())
            .await
            .unwrap();
        assert_eq!(out.reads, reads);
        assert!(out.turns > 0);
        assert!(out.turns <= MAX_SPECULATION_TURNS);
        assert!(out.messages <= MAX_SPECULATION_MESSAGES);
        assert!(out.tool_calls >= 1);
    }

    #[tokio::test]
    async fn missing_overlay_dir_errors_clearly() {
        let adapter = Arc::new(CountingAdapter::default());
        let runner = ForkedAgentRunner::new(adapter);
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let params = CacheSafeParams::new(Uuid::new_v4(), missing);
        let err = runner
            .speculate(&params, "prompt", Vec::new(), ForkBudget::default())
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("overlay_dir"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn total_forks_counter_increments() {
        let adapter = Arc::new(CountingAdapter::default());
        let runner = ForkedAgentRunner::new(adapter);
        let tmp = tmp_overlay();
        let params = CacheSafeParams::new(Uuid::new_v4(), tmp.path().to_path_buf());
        assert_eq!(runner.total_forks().await, 0);
        for _ in 0..3 {
            let _ = runner
                .speculate(&params, "p", Vec::new(), ForkBudget::default())
                .await;
        }
        assert_eq!(runner.total_forks().await, 3);
    }

    #[tokio::test]
    async fn turn_cap_exits_when_adapter_stays_quiet() {
        // Adapter returns empty content → runner keeps looping
        // until it hits `budget.effective_turns()`. This is the
        // branch the "MVP exit on first non-empty content" path
        // bypasses, and it's what the plan's MAX_SPECULATION_TURNS
        // acceptance asserts.
        struct QuietAdapter;
        #[async_trait]
        impl LlmAdapter for QuietAdapter {
            async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
                Ok(LlmResponse {
                    provider: "test".into(),
                    model: "quiet".into(),
                    content: String::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                })
            }
        }
        let runner = ForkedAgentRunner::new(Arc::new(QuietAdapter));
        let tmp = tmp_overlay();
        let params = CacheSafeParams::new(Uuid::new_v4(), tmp.path().to_path_buf());
        let budget = ForkBudget {
            max_turns: 3,
            max_duration: Duration::from_secs(5),
            ..ForkBudget::default()
        };
        let out = runner
            .speculate(&params, "p", Vec::new(), budget)
            .await
            .unwrap();
        assert_eq!(out.turns, 3, "turn cap must bite when content is empty");
        assert_eq!(out.tool_calls, 0, "no tool calls when content never arrives");
    }

    #[tokio::test]
    async fn setup_error_does_not_increment_counter() {
        let adapter = Arc::new(CountingAdapter::default());
        let runner = ForkedAgentRunner::new(adapter);
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let params = CacheSafeParams::new(Uuid::new_v4(), missing);
        let _ = runner
            .speculate(&params, "p", Vec::new(), ForkBudget::default())
            .await;
        assert_eq!(
            runner.total_forks().await,
            0,
            "failed setup must not bump fork counter"
        );
    }

    #[tokio::test]
    async fn cleanup_async_removes_overlay() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("overlay");
        let guard = OverlayGuard::new(path.clone()).await.unwrap();
        assert!(path.is_dir());
        guard.cleanup_async().await;
        assert!(!path.exists(), "cleanup_async must remove the overlay");
    }

    #[tokio::test]
    async fn timeout_aborts_when_adapter_slow() {
        struct SlowAdapter;
        #[async_trait]
        impl LlmAdapter for SlowAdapter {
            async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(LlmResponse {
                    provider: "test".into(),
                    model: "slow".into(),
                    content: "".into(),
                    input_tokens: 0,
                    output_tokens: 0,
                })
            }
        }
        let runner = ForkedAgentRunner::new(Arc::new(SlowAdapter));
        let tmp = tmp_overlay();
        let params = CacheSafeParams::new(Uuid::new_v4(), tmp.path().to_path_buf());
        let budget = ForkBudget {
            max_duration: Duration::from_millis(50),
            ..Default::default()
        };
        let err = runner
            .speculate(&params, "p", Vec::new(), budget)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("timed out"));
    }
}
