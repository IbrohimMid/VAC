//! B2 — `VacToolsDispatcher`: concrete `ToolDispatcher` impl that
//! routes tool-use blocks emitted by the LLM through the live
//! `vac_tools::ToolRegistry`. Pre-B2 the engine fell back to
//! `UnsupportedDispatcher` whenever the driver forgot to wire one;
//! live sessions now ship a real dispatcher by default via the
//! `live_compact_config` builder below.
//!
//! **Placement note**: this lives in `vac_tui_runtime` (not
//! `vac_tools`) because `ToolDispatcher` is defined in
//! `vac_session_engine`; making `vac_tools` depend on session
//! engine would re-introduce the `vac_tools → vac_session_engine
//! → vac_core → vac_tools` cycle that Part 1 of the NS arc broke.
//! `vac_tui_runtime` already depends on both crates.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use vac_session_engine::{
    CompactConfig, EngineError, EngineResult, ToolCallRequest, ToolDispatcher,
};
use vac_tool_core::{ToolResultEnvelope, ToolResultKind};
use vac_tools::registry::{ToolContext, ToolRegistry};

pub struct VacToolsDispatcher {
    registry: Arc<ToolRegistry>,
    ctx: Arc<ToolContext>,
}

impl VacToolsDispatcher {
    pub fn new(registry: Arc<ToolRegistry>, ctx: Arc<ToolContext>) -> Self {
        Self { registry, ctx }
    }
}

#[async_trait]
impl ToolDispatcher for VacToolsDispatcher {
    async fn dispatch(&self, call: &ToolCallRequest) -> EngineResult<ToolResultEnvelope> {
        let started = Instant::now();
        let tool = self.registry.get(&call.name).await.ok_or_else(|| {
            EngineError::Other(format!(
                "VacToolsDispatcher: tool '{}' not registered",
                call.name
            ))
        })?;

        let out = tool.execute(call.arguments.clone(), &self.ctx).await;
        let duration_ms = started.elapsed().as_millis() as u64;

        match out {
            Ok(payload) => Ok(ToolResultEnvelope {
                kind: ToolResultKind::Ok,
                summary: format!("{} ok", call.name),
                payload,
                duration_ms,
            }),
            Err(e) => Ok(ToolResultEnvelope {
                kind: ToolResultKind::Error,
                summary: format!("{} failed", call.name),
                payload: serde_json::json!({ "error": e.to_string() }),
                duration_ms,
            }),
        }
    }
}

/// B2 — assemble a `CompactConfig` with a live dispatcher + gate
/// stack for production driver paths. Pre-B2, live paths passed
/// `CompactConfig::default()` (all slots `None`) and the engine
/// silently fell back to `UnsupportedDispatcher` — tool-use never
/// actually ran. Now live paths call this builder and the wiring
/// is host-required rather than host-optional-by-convention.
///
/// `gate` is an optional pre-built `CompositeGate`. Use
/// [`build_live_gate`] to get one that includes `HookGate`.
pub fn live_compact_config(
    registry: Arc<ToolRegistry>,
    ctx: Arc<ToolContext>,
    gate: Option<Arc<vac_session_engine::CompositeGate>>,
) -> CompactConfig {
    let dispatcher: Arc<dyn ToolDispatcher> = Arc::new(VacToolsDispatcher::new(registry, ctx));
    CompactConfig {
        dispatcher: Some(dispatcher),
        gate,
        ..CompactConfig::default()
    }
}

/// B3 + audit P0.2 — construct the **full** live `CompositeGate`
/// stack. Composes (in deny-wins order):
///
/// 1. `PolicyGate` — vac_core rate/token budget if `policy_tracker`
///    supplied. Skipped when None so headless callers that opt out
///    of budget gating don't pay for it.
/// 2. `PlanModeGate` — enforces the read-only tool allowlist when
///    the caller flips `plan_active`. Skipped when the Arc is
///    None; default TUI flow passes a shared `AtomicBool`.
/// 3. `HookGate` — backed by `HookSandbox::operator_default()`
///    (env allowlist + rlimits + 5-min wall-clock).
///
/// Pre-P0.2 this builder composed *only* `HookGate`. Claims of
/// "live drivers pass full gate+dispatcher" now match code:
/// policy/plan/hooks are all wired when supplied.
///
/// `.vac/hooks.json` missing is fail-soft (empty store). Schema
/// validation surfaces malformed entries as `Err` rather than
/// silent disable.
pub async fn build_live_gate(
    project_root: &std::path::Path,
) -> anyhow::Result<Arc<vac_session_engine::CompositeGate>> {
    build_live_gate_with(project_root, None, None).await
}

/// Full-stack variant where the caller supplies optional policy
/// tracker + plan-mode flag. `build_live_gate` above delegates
/// here with None/None for the common case.
pub async fn build_live_gate_with(
    project_root: &std::path::Path,
    policy_tracker: Option<Arc<vac_core::policy_limits::PolicyTracker>>,
    plan_active: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> anyhow::Result<Arc<vac_session_engine::CompositeGate>> {
    use vac_session_engine::{CompositeGate, PlanModeGate, PolicyGate};
    use vac_session_primitives::{HookSandbox, HookStore, hooks::validate_hook_store};

    let store = HookStore::load(project_root)
        .await
        .map_err(|e| anyhow::anyhow!("load .vac/hooks.json: {e}"))?;
    validate_hook_store(&store).map_err(|e| anyhow::anyhow!(".vac/hooks.json validation: {e}"))?;
    let hook_gate = Arc::new(vac_session_engine::HookGate::with_sandbox(
        store,
        HookSandbox::operator_default(),
    ));

    let mut composite = CompositeGate::new();
    // Order matters — deny-wins + first-hit short-circuits. Check
    // policy caps before hook work because a blown budget should
    // deny immediately without spawning the hook subprocess.
    if let Some(tracker) = policy_tracker {
        composite.push(Arc::new(PolicyGate::new(tracker)));
    }
    if let Some(active) = plan_active {
        composite.push(Arc::new(PlanModeGate::new(active)));
    }
    composite.push(hook_gate);
    Ok(Arc::new(composite))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use vac_tools::error::ToolError;
    use vac_tools::registry::VilTool;

    struct OkTool;
    #[async_trait]
    impl VilTool for OkTool {
        fn spec(&self) -> vac_tool_core::ToolSpec {
            vac_tools::registry::default_spec(self)
        }
        fn name(&self) -> &str {
            "ok_tool"
        }
        fn description(&self) -> &str {
            ""
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn trust_requirement(&self) -> &str {
            "safe"
        }
        fn risk_level(&self) -> &str {
            "safe"
        }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            Ok(serde_json::json!({ "ok": true }))
        }
    }

    struct ErrTool;
    #[async_trait]
    impl VilTool for ErrTool {
        fn spec(&self) -> vac_tool_core::ToolSpec {
            vac_tools::registry::default_spec(self)
        }
        fn name(&self) -> &str {
            "err_tool"
        }
        fn description(&self) -> &str {
            ""
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn trust_requirement(&self) -> &str {
            "safe"
        }
        fn risk_level(&self) -> &str {
            "safe"
        }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<serde_json::Value, ToolError> {
            Err(ToolError::ExecutionFailed("boom".into()))
        }
    }

    async fn build_ctx() -> Arc<ToolContext> {
        Arc::new(ToolContext::new(std::env::temp_dir()))
    }

    #[tokio::test]
    async fn dispatches_registered_tool() {
        let reg = Arc::new(ToolRegistry::new());
        reg.register(OkTool).await.unwrap();
        let d = VacToolsDispatcher::new(reg, build_ctx().await);
        let env = d
            .dispatch(&ToolCallRequest {
                id: "1".into(),
                name: "ok_tool".into(),
                arguments: serde_json::json!({}),
                reason: None,
                estimated_tokens: 0,
            })
            .await
            .unwrap();
        assert_eq!(env.kind, ToolResultKind::Ok);
        assert_eq!(env.payload["ok"], true);
    }

    #[tokio::test]
    async fn wraps_tool_error_as_error_envelope() {
        let reg = Arc::new(ToolRegistry::new());
        reg.register(ErrTool).await.unwrap();
        let d = VacToolsDispatcher::new(reg, build_ctx().await);
        let env = d
            .dispatch(&ToolCallRequest {
                id: "1".into(),
                name: "err_tool".into(),
                arguments: serde_json::json!({}),
                reason: None,
                estimated_tokens: 0,
            })
            .await
            .unwrap();
        assert_eq!(env.kind, ToolResultKind::Error);
        assert!(env.payload["error"].as_str().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn missing_tool_returns_engine_error() {
        let reg = Arc::new(ToolRegistry::new());
        let d = VacToolsDispatcher::new(reg, build_ctx().await);
        let err = d
            .dispatch(&ToolCallRequest {
                id: "1".into(),
                name: "nonexistent".into(),
                arguments: serde_json::json!({}),
                reason: None,
                estimated_tokens: 0,
            })
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not registered"));
    }

    #[tokio::test]
    async fn live_compact_config_installs_dispatcher() {
        let reg = Arc::new(ToolRegistry::new());
        reg.register(OkTool).await.unwrap();
        let cfg = live_compact_config(reg, build_ctx().await, None);
        assert!(cfg.dispatcher.is_some());
        assert!(cfg.gate.is_none()); // explicit None means permissive
    }

    /// Audit M2: missing `.vac/hooks.json` must be fail-soft
    /// (empty store, no error) — first-run projects don't have one
    /// and must still submit.
    #[tokio::test]
    async fn build_live_gate_fail_soft_on_missing_hooks_file() {
        let tmp = tempfile::tempdir().unwrap();
        let gate = build_live_gate(tmp.path()).await.unwrap();
        assert!(
            !gate.is_empty(),
            "HookGate still composed even with no hooks"
        );
    }

    /// Audit M2: malformed hooks.json must surface a crisp
    /// operator-facing error rather than a deep parse panic.
    #[tokio::test]
    async fn build_live_gate_rejects_malformed_hooks_json() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join(".vac");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("hooks.json"), "{ not valid json").unwrap();
        let err = build_live_gate(tmp.path()).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(".vac/hooks.json") || msg.contains("hooks.json"),
            "error should reference hooks.json; got: {msg}",
        );
    }

    /// Audit P0.2 — builder with policy tracker + plan flag
    /// composes a gate stack strictly longer than the
    /// hooks-only default. Length-based check avoids brittling on
    /// internal `CompositeGate` field layout.
    #[tokio::test]
    async fn build_live_gate_with_composes_full_stack() {
        use std::sync::atomic::AtomicBool;
        use vac_core::policy_limits::{PolicyLimits, PolicyTracker};
        let tmp = tempfile::tempdir().unwrap();
        let tracker = Arc::new(PolicyTracker::new(PolicyLimits::default()));
        let plan = Arc::new(AtomicBool::new(false));
        let full = build_live_gate_with(tmp.path(), Some(tracker), Some(plan))
            .await
            .unwrap();
        let hooks_only = build_live_gate(tmp.path()).await.unwrap();
        assert!(
            full.len() > hooks_only.len(),
            "full stack ({}) must have more gates than hooks-only ({})",
            full.len(),
            hooks_only.len(),
        );
        assert_eq!(full.len(), 3, "policy + plan + hooks");
    }

    /// Audit P0.2 reviewer — the shared adapter's shipped path
    /// (what `vac run` + TUI actually execute) must compose the
    /// full gate stack, not fall through to the hooks-only
    /// default wrapper. Since the adapter isn't directly callable
    /// without a VacEngine, we assert the rebuild invariant: the
    /// same PolicyLimits::load() + build_live_gate_with(Some(t), None)
    /// pattern the adapter uses yields a >1-gate composite, proving
    /// the live path is not stuck at hooks-only.
    #[tokio::test]
    async fn live_shared_path_composes_policy_plus_hooks() {
        use vac_core::policy_limits::{PolicyLimits, PolicyTracker};
        let tmp = tempfile::tempdir().unwrap();
        // Mirror the adapter's exact construction.
        let limits = PolicyLimits::load(tmp.path()).await.unwrap();
        let tracker = Arc::new(PolicyTracker::new(limits));
        let shipped_gate = build_live_gate_with(tmp.path(), Some(tracker), None)
            .await
            .unwrap();
        assert!(
            shipped_gate.len() >= 2,
            "shipped live path must include PolicyGate + HookGate; got {} gates",
            shipped_gate.len(),
        );
    }

    /// Audit M2: hook entries that fail schema validation (e.g.
    /// empty id, bad regex matcher) surface a validation error.
    #[tokio::test]
    async fn build_live_gate_rejects_schema_violation() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join(".vac");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        // `matcher` is an invalid regex; `validate_hook_store`
        // should surface it.
        std::fs::write(
            hooks_dir.join("hooks.json"),
            r#"{"entries":[{"id":"h1","event":"PreToolUse","matcher":"(unclosed","type":"command","argv":["true"]}]}"#,
        )
        .unwrap();
        let err = build_live_gate(tmp.path()).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("validation") || msg.contains("regex"),
            "error should mention validation; got: {msg}",
        );
    }
}
