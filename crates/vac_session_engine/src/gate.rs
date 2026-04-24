//! A.2 — `ToolGate` + `CompositeGate` (aka `wrappedCanUseTool`).
//!
//! Every tool invocation inside the streamed submit loop (A.3) has
//! to clear every operator-facing gate before it runs. Today the
//! same concept is spread across five places:
//!
//! - `vac_core::policy_limits::PolicyTracker::check` — rate + token
//!   caps.
//! - `vac_tools::trust_gate::TrustGate` — trust class + isolation
//!   mode decisions.
//! - `vac_approvals::ApprovalStateMachine` — per-call Allow /
//!   NeedsApproval / Reject.
//! - `vac_mcp_core::channel::ChannelAcl` — MCP channel Allow / Deny
//!   / Notify.
//! - `crate::compact::CompactBoundary` — context-ceiling gate.
//!
//! Phase C lands a sixth gate (hook registry: PreToolUse). Each of
//! these has its own decision type + call site; the new streaming
//! submit loop can't afford to re-implement the fan-out.
//!
//! This module defines the *contract* all gates implement and a
//! composer that short-circuits on the first `Deny`. It does NOT
//! itself depend on any of the four downstream crates — each gate
//! wrapper lives in the crate that owns the primitive, and drivers
//! register the adapters they want.
//!
//! A.3 threads `CompositeGate::check` in front of every tool-use
//! block the streamed adapter emits.

use async_trait::async_trait;
use std::sync::Arc;

/// Enough context for a gate to make a decision. More fields land
/// in later phases (subagent scope, hook event, trust class).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolCheckCtx {
    pub tool_name: String,
    /// JSON arguments as the LLM emitted them. Gates that need to
    /// inspect the payload (e.g. file-write gate asking for the
    /// target path) parse this.
    pub arguments: serde_json::Value,
    /// Session id the tool runs within — approvals + policy tracker
    /// scope by this.
    pub session_id: uuid::Uuid,
    /// Short, operator-readable reason why the tool is being
    /// invoked. Used for approval prompts + denial traces.
    pub reason: Option<String>,
    /// Estimated token cost to charge if the call completes; policy
    /// tracker uses this to preempt token-cap breaches.
    pub estimated_tokens: u64,
}

impl ToolCheckCtx {
    pub fn new(tool_name: impl Into<String>, session_id: uuid::Uuid) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments: serde_json::Value::Null,
            session_id,
            reason: None,
            estimated_tokens: 0,
        }
    }

    pub fn with_arguments(mut self, args: serde_json::Value) -> Self {
        self.arguments = args;
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_estimated_tokens(mut self, n: u64) -> Self {
        self.estimated_tokens = n;
        self
    }
}

/// What a gate returns.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GateDecision {
    /// Tool may proceed. `notes` bubble up for the activity panel
    /// without blocking.
    Allow { notes: Vec<String> },
    /// Operator must approve before the tool runs. `reason` shown
    /// in the approval prompt.
    NeedsApproval { reason: String },
    /// Tool is blocked. `reason` surfaces through NotifyRouter.
    Deny { reason: String },
}

impl GateDecision {
    pub fn allow() -> Self {
        Self::Allow { notes: Vec::new() }
    }

    pub fn allow_with_note(note: impl Into<String>) -> Self {
        Self::Allow {
            notes: vec![note.into()],
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    pub fn needs_approval(reason: impl Into<String>) -> Self {
        Self::NeedsApproval {
            reason: reason.into(),
        }
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }

    pub fn is_needs_approval(&self) -> bool {
        matches!(self, Self::NeedsApproval { .. })
    }
}

/// The gate contract.
#[async_trait]
pub trait ToolGate: Send + Sync + std::fmt::Debug {
    /// Short label for tracing. Matches the BRIDGE_ALLOWLIST label
    /// where one exists (`policy`, `trust`, `approval`, `mcp`,
    /// `hooks`) so denials light up with the right subsystem.
    fn label(&self) -> &'static str;

    async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision;
}

/// Composer. Evaluates gates in registration order, short-circuits
/// on first Deny, escalates to NeedsApproval if any gate asked for
/// one (even after Allow notes accumulate), and returns Allow with
/// merged notes otherwise.
#[derive(Debug, Default, Clone)]
pub struct CompositeGate {
    gates: Vec<Arc<dyn ToolGate>>,
}

impl CompositeGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_gate(mut self, gate: Arc<dyn ToolGate>) -> Self {
        self.gates.push(gate);
        self
    }

    pub fn push(&mut self, gate: Arc<dyn ToolGate>) {
        self.gates.push(gate);
    }

    pub fn len(&self) -> usize {
        self.gates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.gates.is_empty()
    }

    /// Evaluate all gates. See trait-level comment for the
    /// deny-wins / approval-escalates / notes-merge contract.
    pub async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision {
        let mut notes: Vec<String> = Vec::new();
        let mut pending_approval: Option<String> = None;
        for gate in &self.gates {
            let decision = gate.check(ctx).await;
            match decision {
                GateDecision::Deny { reason } => {
                    // Trace at warn on a generic composite target
                    // so the A1 bridge picks it up regardless of
                    // which concrete gate denied.
                    tracing::warn!(
                        target: "vac_session_engine::gate",
                        gate = gate.label(),
                        tool = %ctx.tool_name,
                        reason = %reason,
                        "CompositeGate::check deny",
                    );
                    return GateDecision::Deny { reason };
                }
                GateDecision::NeedsApproval { reason } => {
                    // First NeedsApproval wins; subsequent Allow
                    // notes are collected but don't downgrade.
                    if pending_approval.is_none() {
                        pending_approval = Some(reason);
                    }
                }
                GateDecision::Allow { notes: n } => notes.extend(n),
            }
        }
        match pending_approval {
            Some(reason) => GateDecision::NeedsApproval { reason },
            None => GateDecision::Allow { notes },
        }
    }
}

// ── Built-in gate: PolicyTracker ─────────────────────────────────

/// Wraps `vac_core::policy_limits::PolicyTracker::check` as a
/// [`ToolGate`]. The tracker itself lives in vac_core (the engine
/// already depends on it for C1); this adapter keeps the gate
/// composition in vac_session_engine so drivers don't need to
/// re-wire the fan-out.
#[derive(Clone)]
pub struct PolicyGate {
    tracker: Arc<vac_core::policy_limits::PolicyTracker>,
}

impl std::fmt::Debug for PolicyGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyGate").finish_non_exhaustive()
    }
}

impl PolicyGate {
    pub fn new(tracker: Arc<vac_core::policy_limits::PolicyTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl ToolGate for PolicyGate {
    fn label(&self) -> &'static str {
        "policy"
    }

    async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision {
        use vac_core::policy_limits::{PolicyDecision, SubmitIntent};
        let intent = SubmitIntent {
            tool: Some(ctx.tool_name.as_str()),
            additional_tokens: ctx.estimated_tokens,
        };
        match self.tracker.check(&intent).await {
            PolicyDecision::Allow => GateDecision::allow(),
            PolicyDecision::Deny(reason) => GateDecision::Deny { reason },
        }
    }
}

// ── PlanModeGate — B.4 ────────────────────────────────────────────

/// Plan-mode gate. While `active` flips to true, every tool whose
/// name isn't on the read-only allowlist denies with a
/// `"plan_mode"` reason; operator uses `/exit-plan` to flip the
/// flag back off.
///
/// Default read-only allowlist covers the standard research tools
/// (Read / Grep / Glob / LSP / WebFetch-style surfaces). The
/// caller can supply a custom list via [`with_allowed_tools`] so
/// operators with strict plan modes can tighten further or let
/// specific workflow tools through.
#[derive(Debug, Clone)]
pub struct PlanModeGate {
    active: Arc<std::sync::atomic::AtomicBool>,
    allowed: Arc<Vec<String>>,
}

impl PlanModeGate {
    pub fn new(active: Arc<std::sync::atomic::AtomicBool>) -> Self {
        let default_allowed = vec![
            "FileRead".to_string(),
            "Read".to_string(),
            "Grep".to_string(),
            "Glob".to_string(),
            "ToolSearch".to_string(),
            "WebFetch".to_string(),
            "WebSearch".to_string(),
            "CtxInspect".to_string(),
            "Knowledge".to_string(),
            "VilStatus".to_string(),
            "VilDiagnostics".to_string(),
            "VilLspQuery".to_string(),
            "RustSymbolLookup".to_string(),
            "RustDiagnostics".to_string(),
            "Todo".to_string(),
            "SignalList".to_string(),
            "SignalTail".to_string(),
        ];
        Self {
            active,
            allowed: Arc::new(default_allowed),
        }
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed = Arc::new(tools);
        self
    }

    pub fn is_active(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl ToolGate for PlanModeGate {
    fn label(&self) -> &'static str {
        "plan"
    }

    async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision {
        if !self.is_active() {
            return GateDecision::allow();
        }
        let name = ctx.tool_name.as_str();
        if self.allowed.iter().any(|a| a == name) {
            GateDecision::allow_with_note("plan_mode: read-only allowed")
        } else {
            GateDecision::deny(format!(
                "plan mode active — tool '{name}' blocked; use /exit-plan to unlock"
            ))
        }
    }
}

// ── NoopHookGate — placeholder until C.5 ──────────────────────────

/// Placeholder slot Phase C.5 replaces. Today it always allows —
/// A.3 still threads it so the composition shape is stable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopHookGate;

#[async_trait]
impl ToolGate for NoopHookGate {
    fn label(&self) -> &'static str {
        "hooks"
    }

    async fn check(&self, _ctx: &ToolCheckCtx) -> GateDecision {
        GateDecision::allow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[derive(Debug)]
    struct FixedGate {
        name: &'static str,
        decision: GateDecision,
    }

    #[async_trait]
    impl ToolGate for FixedGate {
        fn label(&self) -> &'static str {
            self.name
        }
        async fn check(&self, _ctx: &ToolCheckCtx) -> GateDecision {
            self.decision.clone()
        }
    }

    fn ctx() -> ToolCheckCtx {
        ToolCheckCtx::new("grep", Uuid::new_v4())
    }

    #[tokio::test]
    async fn empty_composite_allows() {
        let cg = CompositeGate::new();
        assert!(matches!(cg.check(&ctx()).await, GateDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn first_deny_wins_and_short_circuits() {
        let cg = CompositeGate::new()
            .with_gate(Arc::new(FixedGate {
                name: "a",
                decision: GateDecision::allow_with_note("a-ok"),
            }))
            .with_gate(Arc::new(FixedGate {
                name: "b",
                decision: GateDecision::deny("b-blocked"),
            }))
            .with_gate(Arc::new(FixedGate {
                name: "c",
                decision: GateDecision::deny("c-also-blocked"),
            }));
        let decision = cg.check(&ctx()).await;
        match decision {
            GateDecision::Deny { reason } => {
                assert_eq!(reason, "b-blocked"); // c never ran
            }
            other => panic!("expected Deny(b-blocked), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn needs_approval_escalates_after_allow() {
        let cg = CompositeGate::new()
            .with_gate(Arc::new(FixedGate {
                name: "a",
                decision: GateDecision::allow_with_note("audit: ok"),
            }))
            .with_gate(Arc::new(FixedGate {
                name: "b",
                decision: GateDecision::needs_approval("approve first"),
            }));
        let decision = cg.check(&ctx()).await;
        match decision {
            GateDecision::NeedsApproval { reason } => {
                assert_eq!(reason, "approve first");
            }
            other => panic!("expected NeedsApproval, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn allow_notes_merge_across_gates() {
        let cg = CompositeGate::new()
            .with_gate(Arc::new(FixedGate {
                name: "a",
                decision: GateDecision::allow_with_note("n1"),
            }))
            .with_gate(Arc::new(FixedGate {
                name: "b",
                decision: GateDecision::allow_with_note("n2"),
            }));
        let decision = cg.check(&ctx()).await;
        match decision {
            GateDecision::Allow { notes } => {
                assert_eq!(notes, vec!["n1", "n2"]);
            }
            other => panic!("expected Allow, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn policy_gate_denies_when_cap_reached() {
        use vac_core::policy_limits::{PolicyLimits, PolicyTracker};
        let tracker = Arc::new(PolicyTracker::new(PolicyLimits {
            max_submits_per_hour: Some(1),
            ..Default::default()
        }));
        let gate = PolicyGate::new(tracker.clone());
        assert!(matches!(
            gate.check(&ctx()).await,
            GateDecision::Allow { .. }
        ));
        tracker.record_submit().await;
        let denied = gate.check(&ctx()).await;
        match denied {
            GateDecision::Deny { reason } => {
                assert!(reason.contains("max_submits_per_hour"), "{reason}");
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn plan_gate_inactive_allows_all() {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let gate = PlanModeGate::new(flag.clone());
        let mut ctx = ctx();
        ctx.tool_name = "Edit".into();
        assert!(matches!(
            gate.check(&ctx).await,
            GateDecision::Allow { .. }
        ));
    }

    #[tokio::test]
    async fn plan_gate_active_denies_destructive_but_allows_read_only() {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let gate = PlanModeGate::new(flag.clone());
        let mut ctx = ctx();
        ctx.tool_name = "Edit".into();
        match gate.check(&ctx).await {
            GateDecision::Deny { reason } => {
                assert!(reason.contains("plan mode"));
            }
            other => panic!("expected Deny, got {other:?}"),
        }
        ctx.tool_name = "Grep".into();
        assert!(matches!(
            gate.check(&ctx).await,
            GateDecision::Allow { .. }
        ));
    }

    #[tokio::test]
    async fn noop_hook_gate_always_allows() {
        let gate = NoopHookGate;
        assert!(matches!(
            gate.check(&ctx()).await,
            GateDecision::Allow { .. }
        ));
    }
}
