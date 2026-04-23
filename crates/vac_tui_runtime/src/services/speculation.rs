//! W1.4 — next-submit speculation.
//!
//! Two strategies behind the `NextSubmitPredictor` trait:
//!
//! - [`HeuristicPredictor`] (P3 default) — rule-based, no LLM, no
//!   tool calls. Cheap + deterministic; serves as the fallback when
//!   the fork driver is unavailable or the budget is exhausted.
//! - [`ForkSpeculationDriver`] (W1.4) — spawns a real
//!   `vac_session_engine::ForkedAgentRunner` against a cache-safe
//!   overlay dir, lets it run read-only tools speculatively, and
//!   exposes the observed reads as a warm cache via the prediction
//!   context.
//!
//! Selection is configuration-driven: see [`SpeculationStrategy`].
//! Driver wires this after `SubmitEvent::Finished`:
//!
//! ```ignore
//! let prediction = predictor.predict(&last_submit, history).await;
//! if let Some(p) = prediction {
//!     state.speculation.set_predicted(p.prompt, p.context);
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use vac_session_engine::{
    CacheSafeParams, ForkBudget, ForkedAgentRunner, LlmAdapter, OverlayGuard,
};

use crate::services::prompt_suggest::PromptHistory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prediction {
    pub prompt: String,
    pub context: HashMap<String, String>,
}

#[async_trait]
pub trait NextSubmitPredictor: Send + Sync {
    async fn predict(
        &self,
        last_submit: &str,
        history: &PromptHistory,
    ) -> Option<Prediction>;
}

/// Heuristic predictor — no LLM. Rules:
///
/// 1. If the last submit matches `/^(write|add)\s+/`, predict
///    `"run the tests for <target>"` as the natural follow-up.
/// 2. If the last submit matches `/fix/i`, predict
///    `"explain the root cause of the issue we just fixed"`.
/// 3. Otherwise: return the most-recent-but-different prompt from
///    the history ring. This is pattern-matching "the operator does
///    the same kind of task again" which is surprisingly common for
///    code-review + refactor cycles.
///
/// Deterministic + cheap; good baseline before a real predictor
/// lands. Returns `None` when none of the rules trigger.
#[derive(Debug, Default, Clone)]
pub struct HeuristicPredictor;

#[async_trait]
impl NextSubmitPredictor for HeuristicPredictor {
    async fn predict(
        &self,
        last_submit: &str,
        history: &PromptHistory,
    ) -> Option<Prediction> {
        let trimmed = last_submit.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lower = trimmed.to_ascii_lowercase();

        // Rule 1: write/add → run the tests
        if lower.starts_with("write ") || lower.starts_with("add ") {
            let target = trimmed
                .splitn(2, char::is_whitespace)
                .nth(1)
                .unwrap_or(trimmed);
            let mut ctx = HashMap::new();
            ctx.insert(
                "hint".into(),
                "Typical follow-up after write/add is to exercise the change.".into(),
            );
            return Some(Prediction {
                prompt: format!("run the tests for {}", target),
                context: ctx,
            });
        }

        // Rule 2: fix → explain root cause
        if lower.contains("fix") {
            let mut ctx = HashMap::new();
            ctx.insert(
                "hint".into(),
                "Root-cause write-up helps the next incident.".into(),
            );
            return Some(Prediction {
                prompt: "explain the root cause of the issue we just fixed".into(),
                context: ctx,
            });
        }

        // Rule 3: next-most-recent-but-different from history.
        // Walk the history from newest → oldest, skip anything
        // matching the current submit.
        for prev in history.suggest("", 8) {
            if prev != trimmed {
                let mut ctx = HashMap::new();
                ctx.insert(
                    "hint".into(),
                    "Operators often re-run a similar kind of task.".into(),
                );
                return Some(Prediction {
                    prompt: prev,
                    context: ctx,
                });
            }
        }
        None
    }
}

/// W1.4 — selects which predictor the driver uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeculationStrategy {
    /// Rule-based heuristic, no LLM. Default because it's free.
    #[default]
    Heuristic,
    /// Fork-based — spawns a real speculative sub-submit.
    Fork,
}

/// W1.4 — fork-based driver. Given a parent's last-submit summary and
/// a rough list of files referenced by the next-likely submit, spawns
/// a `ForkedAgentRunner` against a cache-safe overlay, awaits its
/// `ForkResult`, and surfaces the observed reads to the caller as
/// context chips. On fork abort (budget, timeout, error) returns
/// `None` so the driver can fall back to the heuristic predictor.
pub struct ForkSpeculationDriver {
    runner: ForkedAgentRunner,
    /// Directory under which per-fork overlays are created. Each fork
    /// gets `<root>/<uuid>/`; cleanup is RAII via `OverlayGuard`.
    overlay_root: PathBuf,
    parent_session: Uuid,
    budget: ForkBudget,
}

impl ForkSpeculationDriver {
    pub fn new(
        adapter: Arc<dyn LlmAdapter>,
        overlay_root: PathBuf,
        parent_session: Uuid,
    ) -> Self {
        Self {
            runner: ForkedAgentRunner::new(adapter),
            overlay_root,
            parent_session,
            budget: ForkBudget::default(),
        }
    }

    pub fn with_budget(mut self, budget: ForkBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Run one speculation pass. Returns `Some(Prediction)` when the
    /// fork completed cleanly; `None` on any abort path so the caller
    /// can fall back. The prediction's `context` encodes each observed
    /// read as `@<path>` → short hint — the composer renders those
    /// as warm-context chips.
    ///
    /// Fork errors are not fatal — they are traced at `warn` level
    /// against `vac_tui_runtime::speculation::fork` so operators can
    /// correlate quiet cache misses with overrun budgets or adapter
    /// failures. The async-safe `OverlayGuard::new` keeps this path
    /// on the tokio runtime — no sync filesystem in the hot path.
    #[tracing::instrument(
        target = "vac_tui_runtime::speculation",
        name = "fork.speculate",
        skip_all,
        fields(reads_hint = reads_hint.len()),
    )]
    pub async fn speculate(
        &self,
        next_likely_prompt: &str,
        reads_hint: Vec<PathBuf>,
    ) -> Option<Prediction> {
        if let Err(e) = tokio::fs::create_dir_all(&self.overlay_root).await {
            tracing::warn!(
                target: "vac_tui_runtime::speculation",
                error = %e,
                root = %self.overlay_root.display(),
                "fork overlay root unreachable; falling back",
            );
            return None;
        }
        let overlay_dir = self.overlay_root.join(Uuid::new_v4().to_string());
        let guard = match OverlayGuard::new(overlay_dir.clone()).await {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(
                    target: "vac_tui_runtime::speculation",
                    error = %e,
                    dir = %overlay_dir.display(),
                    "overlay create failed; falling back",
                );
                return None;
            }
        };
        let params = CacheSafeParams::new(self.parent_session, overlay_dir);
        let outcome = match self
            .runner
            .speculate(&params, next_likely_prompt, reads_hint, self.budget.clone())
            .await
        {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(
                    target: "vac_tui_runtime::speculation",
                    error = %e,
                    "fork speculation aborted; falling back",
                );
                // Async cleanup so the Drop sync-rm doesn't fire on
                // the runtime thread.
                guard.cleanup_async().await;
                return None;
            }
        };
        if outcome.reads.is_empty() {
            guard.cleanup_async().await;
            return None;
        }
        let mut ctx = HashMap::new();
        ctx.insert(
            "hint".into(),
            format!(
                "fork warmed {} read(s) in {} turn(s)",
                outcome.reads.len(),
                outcome.turns
            ),
        );
        for path in &outcome.reads {
            ctx.insert(
                format!("@{}", path.display()),
                "warm-read from fork speculation".into(),
            );
        }
        guard.cleanup_async().await;
        Some(Prediction {
            prompt: next_likely_prompt.to_string(),
            context: ctx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_session_engine::{EngineResult, LlmRequest, LlmResponse};

    struct CannedAdapter;
    #[async_trait]
    impl LlmAdapter for CannedAdapter {
        async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
            Ok(LlmResponse {
                provider: "test".into(),
                model: "canned".into(),
                content: "ok".into(),
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }

    #[tokio::test]
    async fn write_rule_predicts_run_tests() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        let out = p.predict("write unit tests for auth", &h).await.unwrap();
        assert!(out.prompt.starts_with("run the tests"));
        assert!(out.context.contains_key("hint"));
    }

    #[tokio::test]
    async fn fix_rule_predicts_root_cause() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        let out = p.predict("fix the panic in session engine", &h).await.unwrap();
        assert!(out.prompt.contains("root cause"));
    }

    #[tokio::test]
    async fn history_fallback_picks_different_prior_prompt() {
        let p = HeuristicPredictor;
        let mut h = PromptHistory::default();
        h.record("refactor module A");
        h.record("audit tests");
        let out = p.predict("audit tests", &h).await.unwrap();
        assert_eq!(out.prompt, "refactor module A");
    }

    #[tokio::test]
    async fn empty_submit_returns_none() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        assert!(p.predict("   ", &h).await.is_none());
    }

    #[tokio::test]
    async fn no_heuristic_no_history_returns_none() {
        let p = HeuristicPredictor;
        let h = PromptHistory::default();
        assert!(p.predict("some random submit", &h).await.is_none());
    }

    // ── W1.4 ForkSpeculationDriver ────────────────────────────────────

    #[tokio::test]
    async fn fork_driver_returns_prediction_with_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let driver = ForkSpeculationDriver::new(
            Arc::new(CannedAdapter),
            tmp.path().to_path_buf(),
            Uuid::new_v4(),
        );
        let reads = vec![PathBuf::from("src/auth/mod.rs")];
        let pred = driver
            .speculate("next likely prompt", reads.clone())
            .await
            .expect("fork should yield prediction");
        assert_eq!(pred.prompt, "next likely prompt");
        assert!(pred.context.contains_key("hint"));
        assert!(pred.context.contains_key("@src/auth/mod.rs"));
    }

    #[tokio::test]
    async fn fork_driver_returns_none_when_reads_hint_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let driver = ForkSpeculationDriver::new(
            Arc::new(CannedAdapter),
            tmp.path().to_path_buf(),
            Uuid::new_v4(),
        );
        // Empty reads_hint; fork will complete but produce no warm
        // context — driver returns None so caller falls back.
        let pred = driver.speculate("p", Vec::new()).await;
        assert!(pred.is_none());
    }

    #[tokio::test]
    async fn fork_driver_cleans_up_overlay_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let driver = ForkSpeculationDriver::new(
            Arc::new(CannedAdapter),
            tmp.path().to_path_buf(),
            Uuid::new_v4(),
        );
        let _ = driver
            .speculate("p", vec![PathBuf::from("a.rs")])
            .await;
        // Overlay root should exist but be empty — each per-fork
        // subdir was RAII-cleaned (and cleanup_async on success path).
        let mut rd = tokio::fs::read_dir(tmp.path()).await.unwrap();
        let mut count = 0usize;
        while let Some(_e) = rd.next_entry().await.unwrap() {
            count += 1;
        }
        assert_eq!(count, 0, "overlay subdirs must be GC'd, found {count}");
    }

    #[tokio::test]
    async fn fork_driver_with_custom_budget_uses_it() {
        use std::time::Duration;
        let tmp = tempfile::tempdir().unwrap();
        let driver = ForkSpeculationDriver::new(
            Arc::new(CannedAdapter),
            tmp.path().to_path_buf(),
            Uuid::new_v4(),
        )
        .with_budget(ForkBudget {
            max_duration: Duration::from_secs(30),
            max_tokens: 16,
            ..ForkBudget::default()
        });
        // CannedAdapter returns 0 tokens so budget doesn't trip —
        // prediction still lands.
        let pred = driver
            .speculate("p", vec![PathBuf::from("x.rs")])
            .await;
        assert!(pred.is_some());
    }
}
